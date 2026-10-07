use regex::{Regex, RegexBuilder};

use super::model::{
    ByteSpan, CaptureGroup, CaptureValue, EvaluationResult, MatchId, RegexFlags, RegexMatch,
    RegexValidationError, SourceIndex, SubstitutionEvaluationResult, SubstitutionResult,
};

/// Compiles a pattern with the tester's supported Rust-regex options.
pub(crate) fn compile_regex(
    pattern: &str,
    flags: &RegexFlags,
) -> Result<Regex, RegexValidationError> {
    let mut builder = RegexBuilder::new(pattern);
    builder
        .case_insensitive(flags.case_insensitive)
        .multi_line(flags.multi_line)
        .dot_matches_new_line(flags.dot_matches_new_line)
        .unicode(flags.unicode)
        .ignore_whitespace(flags.ignore_whitespace);

    builder.build().map_err(|error| RegexValidationError {
        message: error.to_string(),
    })
}

/// Evaluates every non-overlapping match using the Rust `regex` flavor.
///
/// Match and capture spans are end-exclusive UTF-8 byte offsets into `text`.
/// Zero-width matches are retained as empty spans. This evaluator does not
/// impose an arbitrary result cap; interactive workload limits belong to the
/// caller that schedules evaluation.
pub fn evaluate(pattern: &str, flags: &RegexFlags, text: &str) -> EvaluationResult {
    let regex = match compile_regex(pattern, flags) {
        Ok(regex) => regex,
        Err(error) => return EvaluationResult::InvalidPattern(error),
    };

    let source_index = SourceIndex::new(text);
    let capture_names = regex
        .capture_names()
        .map(|name| name.map(str::to_owned))
        .collect::<Vec<_>>();
    let mut matches = Vec::new();

    for captures in regex.captures_iter(text) {
        let full_match = captures
            .get(0)
            .expect("Rust regex capture sets always contain the full match");
        let span = byte_span(full_match.start(), full_match.end());
        let location = source_index
            .location_at(full_match.start())
            .expect("Rust regex match offsets are valid UTF-8 boundaries");

        let groups = capture_names
            .iter()
            .enumerate()
            .skip(1)
            .map(|(group_index, name)| {
                let value = captures
                    .get(group_index)
                    .map(|capture| CaptureValue::Matched {
                        text: capture.as_str().to_owned(),
                        span: byte_span(capture.start(), capture.end()),
                    })
                    .unwrap_or(CaptureValue::Unmatched);

                CaptureGroup {
                    group_index,
                    name: name.clone(),
                    value,
                }
            })
            .collect();

        matches.push(RegexMatch {
            id: MatchId::new(matches.len()),
            text: full_match.as_str().to_owned(),
            span,
            location,
            captures: groups,
        });
    }

    EvaluationResult::Success { matches }
}

/// Replaces every non-overlapping match with Rust-regex capture expansion.
///
/// Replacement syntax is delegated to the engine, including `$1`, `$name`,
/// `${1}` disambiguation, and `$$` for a literal dollar sign. The count is
/// collected from the same replacement traversal, including zero-width
/// matches. Evaluation has no clipboard, storage, or other external effects.
pub fn evaluate_substitution(
    pattern: &str,
    flags: &RegexFlags,
    text: &str,
    replacement: &str,
) -> SubstitutionEvaluationResult {
    let regex = match compile_regex(pattern, flags) {
        Ok(regex) => regex,
        Err(error) => return SubstitutionEvaluationResult::InvalidPattern(error),
    };

    let mut replacements_made = 0;
    let output = regex.replace_all(text, |captures: &regex::Captures<'_>| {
        replacements_made += 1;
        let mut expanded = String::new();
        captures.expand(replacement, &mut expanded);
        expanded
    });

    SubstitutionEvaluationResult::Success(SubstitutionResult {
        output: output.into_owned(),
        replacements_made,
    })
}

fn byte_span(start_byte: usize, end_byte: usize) -> ByteSpan {
    ByteSpan::new(start_byte, end_byte)
        .expect("Rust regex match spans have ordered start and end offsets")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evaluate_matches(pattern: &str, flags: &RegexFlags, text: &str) -> Vec<RegexMatch> {
        match evaluate(pattern, flags, text) {
            EvaluationResult::Success { matches } => matches,
            EvaluationResult::InvalidPattern(error) => {
                panic!("expected a valid regex, got: {}", error.message)
            }
        }
    }

    fn assert_invalid_pattern(pattern: &str) {
        match evaluate(pattern, &RegexFlags::default(), "sample") {
            EvaluationResult::InvalidPattern(error) => assert!(!error.message.is_empty()),
            EvaluationResult::Success { .. } => {
                panic!("expected `{pattern}` to fail Rust regex compilation")
            }
        }
    }

    #[test]
    fn compiles_and_applies_each_supported_rust_regex_flag() {
        let insensitive = RegexFlags {
            case_insensitive: true,
            ..RegexFlags::default()
        };
        assert_eq!(
            evaluate_matches("alpha", &insensitive, "ALPHA")[0].text,
            "ALPHA"
        );

        let multiline = RegexFlags {
            multi_line: true,
            ..RegexFlags::default()
        };
        assert_eq!(
            evaluate_matches("^target$", &multiline, "before\ntarget\nafter")[0].text,
            "target"
        );

        let dot_newline = RegexFlags {
            dot_matches_new_line: true,
            ..RegexFlags::default()
        };
        assert_eq!(
            evaluate_matches("a.b", &dot_newline, "a\nb")[0].text,
            "a\nb"
        );

        let unicode = RegexFlags::default();
        let unicode_matches = evaluate_matches(r"\w+", &unicode, "é e");
        assert_eq!(
            unicode_matches
                .iter()
                .map(|matched| matched.text.as_str())
                .collect::<Vec<_>>(),
            ["é", "e"]
        );

        let ascii = RegexFlags {
            unicode: false,
            ..RegexFlags::default()
        };
        let ascii_matches = evaluate_matches(r"\w+", &ascii, "é e");
        assert_eq!(ascii_matches.len(), 1);
        assert_eq!(ascii_matches[0].text, "e");

        let extended = RegexFlags {
            ignore_whitespace: true,
            ..RegexFlags::default()
        };
        assert_eq!(
            evaluate_matches("foo bar", &extended, "foobar")[0].text,
            "foobar"
        );
    }

    #[test]
    fn returns_all_matches_with_ordered_ids_and_preserves_no_match_results() {
        let flags = RegexFlags::default();
        let matches = evaluate_matches("a", &flags, "aaaa");

        assert_eq!(matches.len(), 4);
        for (index, matched) in matches.iter().enumerate() {
            assert_eq!(matched.id.index(), index);
            assert_eq!(matched.span, ByteSpan::new(index, index + 1).unwrap());
        }

        assert!(evaluate_matches("z", &flags, "abc").is_empty());
    }

    #[test]
    fn retains_numbered_named_and_unmatched_optional_captures() {
        let matches =
            evaluate_matches(r"(?P<first>é)(b)?(?P<last>c)", &RegexFlags::default(), "éc");
        let matched = &matches[0];

        assert_eq!(matched.text, "éc");
        assert_eq!(matched.span, ByteSpan::new(0, 3).unwrap());
        assert_eq!(matched.captures.len(), 3);
        assert_eq!(matched.captures[0].group_index, 1);
        assert_eq!(matched.captures[0].name.as_deref(), Some("first"));
        assert_eq!(
            matched.captures[0].value,
            CaptureValue::Matched {
                text: "é".to_owned(),
                span: ByteSpan::new(0, 2).unwrap(),
            }
        );
        assert_eq!(matched.captures[1].group_index, 2);
        assert_eq!(matched.captures[1].name, None);
        assert_eq!(matched.captures[1].value, CaptureValue::Unmatched);
        assert_eq!(matched.captures[2].group_index, 3);
        assert_eq!(matched.captures[2].name.as_deref(), Some("last"));
        assert_eq!(
            matched.captures[2].value,
            CaptureValue::Matched {
                text: "c".to_owned(),
                span: ByteSpan::new(2, 3).unwrap(),
            }
        );
    }

    #[test]
    fn derives_unicode_byte_spans_and_line_locations_for_lf_and_crlf() {
        let matches = evaluate_matches("é|β", &RegexFlags::default(), "Aé💡\r\nβ\né");

        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].span, ByteSpan::new(1, 3).unwrap());
        assert_eq!(
            matches[0].location,
            super::super::model::SourceLocation { line: 1, column: 2 }
        );
        assert_eq!(matches[1].span, ByteSpan::new(9, 11).unwrap());
        assert_eq!(
            matches[1].location,
            super::super::model::SourceLocation { line: 2, column: 1 }
        );
        assert_eq!(matches[2].span, ByteSpan::new(12, 14).unwrap());
        assert_eq!(
            matches[2].location,
            super::super::model::SourceLocation { line: 3, column: 1 }
        );
    }

    #[test]
    fn keeps_zero_width_matches_at_unicode_character_boundaries() {
        let matches = evaluate_matches("", &RegexFlags::default(), "é");

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].span, ByteSpan::new(0, 0).unwrap());
        assert_eq!(matches[0].location.column, 1);
        assert_eq!(matches[1].span, ByteSpan::new(2, 2).unwrap());
        assert_eq!(matches[1].location.column, 2);
    }

    #[test]
    fn rejects_invalid_patterns_lookaround_and_backreferences_cleanly() {
        assert_invalid_pattern("[");
        assert_invalid_pattern("(?=a)");
        assert_invalid_pattern(r"(a)\1");
    }

    fn substitute(pattern: &str, text: &str, replacement: &str) -> SubstitutionResult {
        match evaluate_substitution(pattern, &RegexFlags::default(), text, replacement) {
            SubstitutionEvaluationResult::Success(result) => result,
            SubstitutionEvaluationResult::InvalidPattern(error) => {
                panic!("expected a valid regex, got: {}", error.message)
            }
        }
    }

    #[test]
    fn substitution_replaces_plain_text_and_all_matches() {
        assert_eq!(
            substitute("cat", "a cat", "dog"),
            SubstitutionResult {
                output: "a dog".to_owned(),
                replacements_made: 1,
            }
        );
        assert_eq!(
            substitute("cat", "cat and cat", "dog"),
            SubstitutionResult {
                output: "dog and dog".to_owned(),
                replacements_made: 2,
            }
        );
    }

    #[test]
    fn substitution_expands_numbered_captures_and_braced_disambiguation() {
        assert_eq!(
            substitute(r"(\w+)-(\d+)", "item-42", "$2/$1").output,
            "42/item"
        );
        assert_eq!(substitute("(a)", "a", "${1}suffix").output, "asuffix");
        // Unbraced names consume the longest valid name, as in Rust regex.
        assert_eq!(substitute("(a)", "a", "$1suffix").output, "");
    }

    #[test]
    fn substitution_expands_named_and_unmatched_optional_captures() {
        assert_eq!(
            substitute(r"(?P<key>\w+)=(?P<value>\d+)", "count=42", "$value:$key").output,
            "42:count"
        );
        assert_eq!(substitute("(a)(b)?", "a", "$1/$2/$missing").output, "a//");
    }

    #[test]
    fn substitution_preserves_literal_dollars_and_backslashes() {
        assert_eq!(
            substitute("(a)", "a", r"$$1 $$ ${1} \1").output,
            r"$1 $ a \1"
        );
    }

    #[test]
    fn substitution_preserves_input_when_no_match_exists() {
        assert_eq!(
            substitute("z", "é abc", "$1"),
            SubstitutionResult {
                output: "é abc".to_owned(),
                replacements_made: 0,
            }
        );
    }

    #[test]
    fn substitution_reports_invalid_patterns_and_reuses_flags() {
        match evaluate_substitution("[", &RegexFlags::default(), "abc", "x") {
            SubstitutionEvaluationResult::InvalidPattern(error) => {
                assert!(!error.message.is_empty())
            }
            SubstitutionEvaluationResult::Success(_) => panic!("expected an invalid pattern"),
        }
        let flags = RegexFlags {
            case_insensitive: true,
            ..RegexFlags::default()
        };
        assert_eq!(
            evaluate_substitution("a", &flags, "A", "x"),
            SubstitutionEvaluationResult::Success(SubstitutionResult {
                output: "x".to_owned(),
                replacements_made: 1,
            })
        );
    }

    #[test]
    fn substitution_counts_zero_width_replacements_using_engine_iteration() {
        for (pattern, text, expected) in [("", "é", "-é-"), ("a*", "aé", "-é-"), ("", "", "-")]
        {
            let result = substitute(pattern, text, "-");
            assert_eq!(result.output, expected);
            assert_eq!(
                result.replacements_made,
                evaluate_matches(pattern, &RegexFlags::default(), text).len()
            );
        }
    }
}
