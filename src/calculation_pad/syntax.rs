//! Logical statements mapped back to their original physical source rows.

use std::ops::Range;

use super::{DiagnosticKind, MAX_EXPRESSION_NESTING};

#[derive(Debug)]
pub(crate) struct SourceRow {
    pub(crate) range: Range<usize>,
    pub(crate) owner: Option<usize>,
    pub(crate) text: String,
    /// Original byte offset for each byte of text, or `None` for a separator.
    pub(crate) offsets: Vec<Option<usize>>,
    pub(super) diagnostic: Option<DiagnosticKind>,
}

#[derive(Debug)]
pub(crate) struct SourceAnalysis {
    pub(crate) rows: Vec<SourceRow>,
    pub(crate) protected: Vec<ProtectedSpan>,
    pub(crate) pairs: Vec<(usize, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtectedKind {
    Comment,
    Quoted,
    Braced,
}

#[derive(Debug)]
pub(crate) struct ProtectedSpan {
    pub(crate) range: Range<usize>,
    pub(crate) kind: ProtectedKind,
}

impl SourceAnalysis {
    pub(crate) fn new(source: &str) -> Self {
        let mut rows: Vec<SourceRow> = Vec::new();
        let mut protected = Vec::new();
        let mut pairs = Vec::new();
        let mut offset = 0;
        let mut owner: Option<usize> = None;
        let mut delimiters: Vec<(u8, usize, usize)> = Vec::new();

        for (line_index, raw) in source.split('\n').enumerate() {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            let range = offset..offset + line.len();
            if let Some(start) = owner {
                let original = &source[rows[start].range.clone()];
                let owner_indent = original.len() - original.trim_start().len();
                let current_indent = line.len() - line.trim_start().len();
                let new_assignment = current_indent <= owner_indent
                    && super::find_assignment_delimiter(line).is_some_and(|delimiter| {
                        super::parse_assignment_name(line[..delimiter.left_end].trim()).is_ok()
                    });
                if new_assignment {
                    if let Some((_, opening_line, column)) = delimiters.last() {
                        rows[start].diagnostic = Some(location_error("unclosed delimiter", *opening_line, *column));
                    }
                    delimiters.clear();
                    owner = None;
                }
            }
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
            let mut braced = None;
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
                        protected.push(ProtectedSpan { range: offset + quote.unwrap()..offset + cursor + 1, kind: ProtectedKind::Quoted });
                        quote = None;
                    }
                } else if let Some(start) = braced {
                    if byte == b'}' {
                        protected.push(ProtectedSpan { range: offset + start..offset + cursor + 1, kind: ProtectedKind::Braced });
                        braced = None;
                    }
                } else {
                    match byte {
                        b'{' => braced = Some(cursor),
                        b'"' => quote = Some(cursor),
                        b'#' => {
                            end = cursor;
                            protected.push(ProtectedSpan { range: offset + cursor..range.end, kind: ProtectedKind::Comment });
                            break;
                        }
                        b'/' if bytes.get(cursor + 1) == Some(&b'/') => {
                            end = cursor;
                            protected.push(ProtectedSpan { range: offset + cursor..range.end, kind: ProtectedKind::Comment });
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
                            if let Some((open, open_line, open_column)) = delimiters.pop().filter(|entry| entry.0 == expected) {
                                let _ = open;
                                pairs.push((rows[open_line].range.start + open_column, offset + cursor));
                            } else {
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
                protected.push(ProtectedSpan { range: offset + start..range.end, kind: ProtectedKind::Quoted });
                error = Some(location_error(
                    "unterminated quoted literal",
                    line_index,
                    start,
                ));
            }
            if let Some(start) = braced {
                protected.push(ProtectedSpan { range: offset + start..range.end, kind: ProtectedKind::Braced });
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
        Self { rows, protected, pairs }
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

    #[test]
    fn recovers_unclosed_statement_at_new_assignment_without_losing_grouped_lines() {
        let source = "for x in range(0):\r\n    bad = (\r\nafter = bad\r\nindependent = 7\r\nvalue = (1 +\r\n2)\r\n{α#} = 3";
        let analysis = SourceAnalysis::new(source);
        assert!(format!("{:?}", analysis.rows[1].diagnostic).contains("unclosed delimiter at line 2"));
        assert_eq!(analysis.rows[2].owner, None);
        assert_eq!(analysis.rows[3].owner, None);
        assert_eq!(analysis.rows[5].owner, Some(4));
        assert_eq!(analysis.rows[4].text, "value = (1 + 2)");
        assert_eq!(analysis.rows[6].owner, None);
        assert_eq!(&source[analysis.rows[3].range.clone()], "independent = 7");
    }
}
