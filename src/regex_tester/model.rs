use super::policy::{EvaluationLimit, EvaluationSuspension};
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// Options supported by the Rust `regex` engine.
///
/// The fields describe regex behavior directly and are independent of any UI
/// controls. The suffix uses the conventional `i`, `m`, `s`, `u`, and `x`
/// letters in that order; Rust regex evaluates all matches without a `g` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RegexFlags {
    pub case_insensitive: bool,
    pub multi_line: bool,
    pub dot_matches_new_line: bool,
    pub unicode: bool,
    pub ignore_whitespace: bool,
}

impl Default for RegexFlags {
    fn default() -> Self {
        Self {
            case_insensitive: false,
            multi_line: false,
            dot_matches_new_line: false,
            unicode: true,
            ignore_whitespace: false,
        }
    }
}

impl RegexFlags {
    /// Returns enabled Rust-regex flags in the familiar `imsux` order.
    pub fn suffix(self) -> String {
        let mut suffix = String::with_capacity(5);
        if self.case_insensitive {
            suffix.push('i');
        }
        if self.multi_line {
            suffix.push('m');
        }
        if self.dot_matches_new_line {
            suffix.push('s');
        }
        if self.unicode {
            suffix.push('u');
        }
        if self.ignore_whitespace {
            suffix.push('x');
        }
        suffix
    }
}

/// Editable inputs for the tester's single text document and replacement.
///
/// This draft intentionally does not implement serialization: history stores
/// only a pattern and flags, while sample text is persisted only by an
/// explicitly saved preset.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RegexDraft {
    pub pattern: String,
    pub flags: RegexFlags,
    pub test_text: String,
    pub replacement: String,
}

/// An end-exclusive UTF-8 byte span in the original test text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByteSpan {
    start_byte: usize,
    end_byte: usize,
}

impl ByteSpan {
    /// Creates a span if the end does not precede the start.
    pub const fn new(start_byte: usize, end_byte: usize) -> Option<Self> {
        if start_byte <= end_byte {
            Some(Self {
                start_byte,
                end_byte,
            })
        } else {
            None
        }
    }

    pub const fn start_byte(self) -> usize {
        self.start_byte
    }

    pub const fn end_byte(self) -> usize {
        self.end_byte
    }

    pub const fn len_bytes(self) -> usize {
        self.end_byte - self.start_byte
    }

    pub const fn is_empty(self) -> bool {
        self.start_byte == self.end_byte
    }

    pub fn as_range(self) -> Range<usize> {
        self.start_byte..self.end_byte
    }
}

/// One-based source location. `column` counts Unicode scalar values, not bytes
/// or grapheme clusters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceLine {
    start_byte: usize,
    content_end_byte: usize,
    scalar_start: usize,
    scalar_end: usize,
}

/// Precomputed source positions for repeated byte-offset-to-location lookups.
///
/// LF and CRLF are each treated as one line ending. Lone CR is also treated as
/// a line ending. Columns count Unicode scalar values before the byte offset,
/// and offsets inside a UTF-8 code point are rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceIndex<'a> {
    text: &'a str,
    lines: Vec<SourceLine>,
    scalar_starts: Vec<usize>,
}

impl<'a> SourceIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut lines = vec![SourceLine {
            start_byte: 0,
            content_end_byte: text.len(),
            scalar_start: 0,
            scalar_end: 0,
        }];
        let mut scalar_starts = Vec::new();
        let mut previous_was_cr = false;

        for (byte_offset, ch) in text.char_indices() {
            match ch {
                '\r' => {
                    let line = lines
                        .last_mut()
                        .expect("the source index always contains an initial line");
                    line.content_end_byte = byte_offset;
                    line.scalar_end = scalar_starts.len();

                    let crlf = text.as_bytes().get(byte_offset + 1) == Some(&b'\n');
                    let next_line_start = byte_offset + if crlf { 2 } else { 1 };
                    lines.push(SourceLine {
                        start_byte: next_line_start,
                        content_end_byte: text.len(),
                        scalar_start: scalar_starts.len(),
                        scalar_end: scalar_starts.len(),
                    });
                    previous_was_cr = true;
                }
                '\n' if previous_was_cr => {
                    previous_was_cr = false;
                }
                '\n' => {
                    let line = lines
                        .last_mut()
                        .expect("the source index always contains an initial line");
                    line.content_end_byte = byte_offset;
                    line.scalar_end = scalar_starts.len();
                    lines.push(SourceLine {
                        start_byte: byte_offset + 1,
                        content_end_byte: text.len(),
                        scalar_start: scalar_starts.len(),
                        scalar_end: scalar_starts.len(),
                    });
                    previous_was_cr = false;
                }
                _ => {
                    scalar_starts.push(byte_offset);
                    previous_was_cr = false;
                }
            }
        }

        if let Some(line) = lines.last_mut() {
            line.scalar_end = scalar_starts.len();
        }

        Self {
            text,
            lines,
            scalar_starts,
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Derives a one-based line and Unicode-scalar column for a byte offset.
    ///
    /// Returns `None` when the offset is beyond the text or is not a UTF-8
    /// character boundary.
    pub fn location_at(&self, byte_offset: usize) -> Option<SourceLocation> {
        if byte_offset > self.text.len() || !self.text.is_char_boundary(byte_offset) {
            return None;
        }

        let line_index = self
            .lines
            .partition_point(|line| line.start_byte <= byte_offset)
            .saturating_sub(1);
        let line = self.lines.get(line_index)?;
        let column_end = byte_offset.min(line.content_end_byte);
        let line_scalars = &self.scalar_starts[line.scalar_start..line.scalar_end];
        let scalars_before_offset =
            line_scalars.partition_point(|start_byte| *start_byte < column_end);

        Some(SourceLocation {
            line: line_index + 1,
            column: scalars_before_offset + 1,
        })
    }
}

/// Identity of a match within one evaluation result.
///
/// The index is zero-based and should be discarded when that evaluation is
/// replaced by a newer result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchId(usize);

impl MatchId {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

/// A capture's value and original test-text span, when the group participated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureValue {
    Unmatched,
    Matched { text: String, span: ByteSpan },
}

/// A numbered capture group, optionally carrying its Rust-regex group name.
///
/// `group_index` follows the regex engine's numbering: group zero is the full
/// match, and capturing groups begin at one. Match records contain only
/// capturing groups, so their indices begin at one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureGroup {
    pub group_index: usize,
    pub name: Option<String>,
    pub value: CaptureValue,
}

/// One complete regex match and its captures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexMatch {
    pub id: MatchId,
    pub text: String,
    pub span: ByteSpan,
    pub location: SourceLocation,
    pub captures: Vec<CaptureGroup>,
}

/// Inline pattern-validation feedback from the Rust regex compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexValidationError {
    pub message: String,
}

/// Structured pattern evaluation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluationResult {
    Success {
        matches: Vec<RegexMatch>,
        completeness: MatchCompleteness,
    },
    InvalidPattern(RegexValidationError),
    Suspended(EvaluationSuspension),
}

/// Whether all matches were enumerated. Truncated results contain only whole
/// rows; `at_least` includes the first known match that could not be stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchCompleteness {
    Complete,
    Truncated {
        reason: EvaluationLimit,
        at_least: usize,
    },
}

/// Successful local substitution output and the number of replacements made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubstitutionResult {
    pub output: String,
    pub replacements_made: usize,
}

/// Structured substitution state, including pattern compilation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubstitutionEvaluationResult {
    Success(SubstitutionResult),
    InvalidPattern(RegexValidationError),
    Suspended(EvaluationSuspension),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_default_to_unicode_and_format_enabled_suffixes_in_canonical_order() {
        let defaults = RegexFlags::default();
        assert!(defaults.unicode);
        assert_eq!(defaults.suffix(), "u");

        let flags = RegexFlags {
            case_insensitive: true,
            multi_line: true,
            dot_matches_new_line: true,
            unicode: true,
            ignore_whitespace: true,
        };
        assert_eq!(flags.suffix(), "imsux");
    }

    #[test]
    fn source_index_counts_unicode_scalars_and_treats_crlf_as_one_line_ending() {
        let text = "Aé💡\r\nβ\nz";
        let index = SourceIndex::new(text);

        assert_eq!(index.line_count(), 3);
        assert_eq!(
            index.location_at(3),
            Some(SourceLocation { line: 1, column: 3 })
        );
        assert_eq!(
            index.location_at(7),
            Some(SourceLocation { line: 1, column: 4 })
        );
        assert_eq!(
            index.location_at(8),
            Some(SourceLocation { line: 1, column: 4 })
        );
        assert_eq!(
            index.location_at(9),
            Some(SourceLocation { line: 2, column: 1 })
        );
        assert_eq!(
            index.location_at(12),
            Some(SourceLocation { line: 3, column: 1 })
        );
        assert_eq!(
            index.location_at(text.len()),
            Some(SourceLocation { line: 3, column: 2 })
        );
    }

    #[test]
    fn source_index_handles_lf_empty_lines_and_rejects_invalid_offsets() {
        let text = "first\n\nthird";
        let index = SourceIndex::new(text);

        assert_eq!(index.line_count(), 3);
        assert_eq!(
            index.location_at(6),
            Some(SourceLocation { line: 2, column: 1 })
        );
        assert_eq!(
            index.location_at(7),
            Some(SourceLocation { line: 3, column: 1 })
        );
        assert_eq!(
            index.location_at(text.len()),
            Some(SourceLocation { line: 3, column: 6 })
        );

        let unicode_index = SourceIndex::new("é");
        assert_eq!(unicode_index.location_at(1), None);
        assert_eq!(unicode_index.location_at(3), None);
    }

    #[test]
    fn byte_spans_preserve_empty_and_end_exclusive_ranges() {
        let span = ByteSpan::new(2, 5).unwrap();
        assert_eq!(span.as_range(), 2..5);
        assert_eq!(span.len_bytes(), 3);
        assert!(!span.is_empty());

        let empty = ByteSpan::new(5, 5).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.len_bytes(), 0);
        assert_eq!(ByteSpan::new(6, 5), None);
    }
}
