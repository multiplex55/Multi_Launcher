//! Logical statements mapped back to their original physical source rows.

use std::ops::Range;

use super::{DiagnosticKind, MAX_EXPRESSION_NESTING};

#[derive(Debug)]
pub(super) struct SourceRow {
    pub(super) range: Range<usize>,
    pub(super) owner: Option<usize>,
    pub(super) text: String,
    /// Original byte offset for each byte of text, or `None` for a separator.
    pub(super) offsets: Vec<Option<usize>>,
    pub(super) diagnostic: Option<DiagnosticKind>,
}

#[derive(Debug)]
pub(super) struct SourceAnalysis {
    pub(super) rows: Vec<SourceRow>,
}

impl SourceAnalysis {
    pub(super) fn new(source: &str) -> Self {
        let mut rows: Vec<SourceRow> = Vec::new();
        let mut offset = 0;
        let mut owner = None;
        let mut delimiters: Vec<(u8, usize, usize)> = Vec::new();

        for (line_index, raw) in source.split('\n').enumerate() {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            let range = offset..offset + line.len();
            let statement = owner.unwrap_or(line_index);
            rows.push(SourceRow {
                range: range.clone(),
                owner,
                text: String::new(),
                offsets: Vec::new(),
                diagnostic: None,
            });

            let bytes = line.as_bytes();
            let mut cursor = 0;
            let mut end = bytes.len();
            let mut braced = false;
            let mut quote = None;
            let mut escaped = false;
            let mut error = None;
            while cursor < bytes.len() {
                let byte = bytes[cursor];
                if quote.is_some() {
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        quote = None;
                    }
                } else if braced {
                    if byte == b'}' {
                        braced = false;
                    }
                } else {
                    match byte {
                        b'{' => braced = true,
                        b'"' => quote = Some(cursor),
                        b'#' => {
                            end = cursor;
                            break;
                        }
                        b'/' if bytes.get(cursor + 1) == Some(&b'/') => {
                            end = cursor;
                            break;
                        }
                        b'(' | b'[' => {
                            delimiters.push((byte, line_index, cursor));
                            if delimiters.len() > MAX_EXPRESSION_NESTING {
                                error = Some(DiagnosticKind::NestingTooDeep {
                                    max_depth: MAX_EXPRESSION_NESTING,
                                });
                                break;
                            }
                        }
                        b')' | b']' => {
                            let expected = if byte == b')' { b'(' } else { b'[' };
                            if delimiters.pop().map(|entry| entry.0) != Some(expected) {
                                error = Some(location_error(
                                    "mismatched closing delimiter",
                                    line_index,
                                    cursor,
                                ));
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                cursor += 1;
            }
            if let Some(start) = quote {
                error = Some(location_error(
                    "unterminated quoted literal",
                    line_index,
                    start,
                ));
            }

            let target = &mut rows[statement];
            if owner.is_some() {
                target.text.push(' ');
                target.offsets.push(None);
            }
            target.text.push_str(&line[..end]);
            target
                .offsets
                .extend((range.start..range.start + end).map(Some));
            if let Some(error) = error {
                target.diagnostic = Some(error);
                delimiters.clear();
            }
            owner = (!delimiters.is_empty()).then_some(statement);
            offset += raw.len() + 1;
        }

        if let (Some(owner), Some((_, line, column))) = (owner, delimiters.last()) {
            rows[owner].diagnostic = Some(location_error("unclosed delimiter", *line, *column));
        }
        Self { rows }
    }
}

fn location_error(message: &str, line: usize, column: usize) -> DiagnosticKind {
    DiagnosticKind::Script(format!(
        "{message} at line {}, byte {}",
        line + 1,
        column + 1
    ))
}

#[cfg(test)]
mod tests {
    use super::SourceAnalysis;

    #[test]
    fn protects_names_and_literals_and_maps_joined_bytes() {
        let source = "{α#//} = (\r\n  call(\"# // \\\"\") // comment\r\n  + 2) # tail";
        let analysis = SourceAnalysis::new(source);
        assert_eq!(analysis.rows.len(), 3);
        assert_eq!(analysis.rows[1].owner, Some(0));
        assert_eq!(analysis.rows[2].owner, Some(0));
        let statement = &analysis.rows[0];
        assert!(statement.text.contains("{α#//}"));
        assert!(statement.text.contains("\"# // \\\"\""));
        assert!(!statement.text.contains("comment"));
        assert!(!statement.text.contains("tail"));
        assert_eq!(statement.text.len(), statement.offsets.len());
        for (index, original) in statement.offsets.iter().enumerate() {
            if let Some(original) = original {
                assert_eq!(
                    statement.text.as_bytes()[index],
                    source.as_bytes()[*original]
                );
            }
        }
    }

    #[test]
    fn reports_unclosed_or_mismatched_delimiters_at_physical_location() {
        let unclosed = SourceAnalysis::new("α = (1\n  + 2");
        assert!(format!("{:?}", unclosed.rows[0].diagnostic).contains("line 1, byte 6"));
        let mismatched = SourceAnalysis::new("x = (1\n  + 2]");
        assert!(format!("{:?}", mismatched.rows[0].diagnostic).contains("line 2, byte 6"));
    }
}
