use regex::{Regex, RegexBuilder};
use std::collections::HashMap;

use super::model::{
    ByteSpan, CaptureGroup, CaptureValue, EvaluationResult, MatchCompleteness, MatchId, RegexFlags,
    RegexMatch, RegexValidationError, SourceIndex, SubstitutionEvaluationResult,
    SubstitutionResult,
};
use super::policy::{EvaluationLimit, EvaluationPolicy, EvaluationSuspension, check_limit};

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

/// Evaluates non-overlapping matches under the default interactive policy.
///
/// Match and capture spans are end-exclusive UTF-8 byte offsets into `text`.
/// Zero-width matches are retained as empty spans. Truncation stores complete
/// rows and exposes a lower bound rather than pretending to know the total.
pub fn evaluate(pattern: &str, flags: &RegexFlags, text: &str) -> EvaluationResult {
    evaluate_with_policy(pattern, flags, text, &EvaluationPolicy::default())
}

pub fn evaluate_with_policy(
    pattern: &str,
    flags: &RegexFlags,
    text: &str,
    policy: &EvaluationPolicy,
) -> EvaluationResult {
    if let Err(reason) = policy.check_inputs(pattern, text) {
        return EvaluationResult::Suspended(reason);
    }
    let regex = match compile_regex(pattern, flags) {
        Ok(regex) => regex,
        Err(error) => return EvaluationResult::InvalidPattern(error),
    };
    if let Err(reason) = check_limit(
        EvaluationLimit::CaptureGroups,
        policy.capture_groups,
        regex.captures_len() - 1,
    ) {
        return EvaluationResult::Suspended(reason);
    }

    let source_index = SourceIndex::new(text);
    let capture_names = regex
        .capture_names()
        .map(|name| name.map(str::to_owned))
        .collect::<Vec<_>>();
    let mut matches = Vec::new();
    let mut materialized_bytes = 0usize;
    let mut completeness = MatchCompleteness::Complete;

    for captures in regex.captures_iter(text) {
        if matches.len() == policy.stored_matches {
            completeness = MatchCompleteness::Truncated {
                reason: EvaluationLimit::StoredMatches,
                at_least: matches.len() + 1,
            };
            break;
        }
        let full_match = captures
            .get(0)
            .expect("Rust regex capture sets always contain the full match");
        // Account for every cloned string before materializing any part of a
        // row, preserving all participating and unmatched captures together.
        let row_bytes = captures
            .iter()
            .flatten()
            .map(|capture| capture.len())
            .chain(capture_names.iter().skip(1).flatten().map(String::len))
            .fold(0usize, usize::saturating_add);
        let candidate_bytes = materialized_bytes.saturating_add(row_bytes);
        if candidate_bytes > policy.materialized_bytes {
            completeness = MatchCompleteness::Truncated {
                reason: EvaluationLimit::MaterializedBytes,
                at_least: matches.len() + 1,
            };
            break;
        }
        materialized_bytes = candidate_bytes;
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

    EvaluationResult::Success {
        matches,
        completeness,
    }
}

/// Replaces all non-overlapping matches within the default interactive limits.
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
    evaluate_substitution_with_policy(
        pattern,
        flags,
        text,
        replacement,
        &EvaluationPolicy::default(),
    )
}

/// Produces either a complete bounded result or explicit suspension, never a
/// partial successful replacement. Interpolation preflight uses the same
/// capture-reference parser as Rust regex, before expanding any capture text.
pub fn evaluate_substitution_with_policy(
    pattern: &str,
    flags: &RegexFlags,
    text: &str,
    replacement: &str,
    policy: &EvaluationPolicy,
) -> SubstitutionEvaluationResult {
    if let Err(reason) = policy.check_inputs(pattern, text).and_then(|()| {
        check_limit(
            EvaluationLimit::ReplacementBytes,
            policy.replacement_bytes,
            replacement.len(),
        )
    }) {
        return SubstitutionEvaluationResult::Suspended(reason);
    }
    let regex = match compile_regex(pattern, flags) {
        Ok(regex) => regex,
        Err(error) => return SubstitutionEvaluationResult::InvalidPattern(error),
    };
    if let Err(reason) = check_limit(
        EvaluationLimit::CaptureGroups,
        policy.capture_groups,
        regex.captures_len() - 1,
    ) {
        return SubstitutionEvaluationResult::Suspended(reason);
    }

    let names: HashMap<_, _> = regex
        .capture_names()
        .enumerate()
        .filter_map(|(index, name)| name.map(|name| (name, index)))
        .collect();
    let mut output = String::new();
    let mut last_end = 0;
    let mut replacements_made = 0usize;
    for captures in regex.captures_iter(text) {
        if replacements_made == policy.stored_matches {
            return SubstitutionEvaluationResult::Suspended(EvaluationSuspension {
                limit: EvaluationLimit::StoredMatches,
                maximum: policy.stored_matches,
                observed: replacements_made + 1,
            });
        }
        let full_match = captures
            .get(0)
            .expect("Rust regex capture sets contain the full match");
        let prefix = &text[last_end..full_match.start()];
        if let Err(reason) = append_output(&mut output, prefix, policy) {
            return SubstitutionEvaluationResult::Suspended(reason);
        }

        // With no capture appends, this temporary string contains only literal
        // replacement bytes, bounded by the already checked replacement input.
        let mut literal = String::new();
        let mut capture_bytes = 0usize;
        regex_automata::util::interpolate::string(
            replacement,
            |index, _| {
                if let Some(capture) = captures.get(index) {
                    capture_bytes = capture_bytes.saturating_add(capture.len());
                }
            },
            |name| names.get(name).copied(),
            &mut literal,
        );
        let expanded_end = output
            .len()
            .saturating_add(literal.len())
            .saturating_add(capture_bytes);
        if let Err(reason) = check_limit(
            EvaluationLimit::ReplacementOutputBytes,
            policy.replacement_output_bytes,
            expanded_end,
        ) {
            return SubstitutionEvaluationResult::Suspended(reason);
        }
        // Exact Rust expansion now has a proven size bound, including repeated
        // references to a large capture and unknown/unmatched references.
        captures.expand(replacement, &mut output);
        last_end = full_match.end();
        replacements_made += 1;
    }
    if let Err(reason) = append_output(&mut output, &text[last_end..], policy) {
        return SubstitutionEvaluationResult::Suspended(reason);
    }

    SubstitutionEvaluationResult::Success(SubstitutionResult {
        output,
        replacements_made,
    })
}

fn append_output(
    output: &mut String,
    text: &str,
    policy: &EvaluationPolicy,
) -> Result<(), EvaluationSuspension> {
    check_limit(
        EvaluationLimit::ReplacementOutputBytes,
        policy.replacement_output_bytes,
        output.len().saturating_add(text.len()),
    )?;
    output.push_str(text);
    Ok(())
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
            EvaluationResult::Success {
                matches,
                completeness: MatchCompleteness::Complete,
            } => matches,
            EvaluationResult::InvalidPattern(error) => {
                panic!("expected a valid regex, got: {}", error.message)
            }
            other => panic!("unexpected bounded evaluation state: {other:?}"),
        }
    }

    fn assert_invalid_pattern(pattern: &str) {
        match evaluate(pattern, &RegexFlags::default(), "sample") {
            EvaluationResult::InvalidPattern(error) => assert!(!error.message.is_empty()),
            EvaluationResult::Success { .. } => {
                panic!("expected `{pattern}` to fail Rust regex compilation")
            }
            EvaluationResult::Suspended(reason) => panic!("unexpected suspension: {reason:?}"),
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
            SubstitutionEvaluationResult::Suspended(reason) => {
                panic!("unexpected suspension: {reason:?}")
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
            SubstitutionEvaluationResult::Suspended(reason) => {
                panic!("unexpected suspension: {reason:?}")
            }
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

    #[test]
    fn input_and_capture_group_budgets_suspend_separately_from_invalid_patterns() {
        let flags = RegexFlags::default();
        let policy = EvaluationPolicy {
            pattern_bytes: 3,
            text_bytes: 2,
            capture_groups: 1,
            ..EvaluationPolicy::default()
        };
        assert!(matches!(
            evaluate_with_policy("aaaa", &flags, "", &policy),
            EvaluationResult::Suspended(EvaluationSuspension {
                limit: EvaluationLimit::PatternBytes,
                maximum: 3,
                observed: 4
            })
        ));
        assert!(matches!(
            evaluate_with_policy("é", &flags, "éa", &policy),
            EvaluationResult::Suspended(EvaluationSuspension {
                limit: EvaluationLimit::TextBytes,
                maximum: 2,
                observed: 3
            })
        ));
        assert!(matches!(
            evaluate_with_policy("é", &flags, "é", &policy),
            EvaluationResult::Success {
                completeness: MatchCompleteness::Complete,
                ..
            }
        ));
        let policy = EvaluationPolicy {
            capture_groups: 1,
            ..EvaluationPolicy::default()
        };
        assert!(matches!(
            evaluate_with_policy("(a)(b)", &flags, "ab", &policy),
            EvaluationResult::Suspended(EvaluationSuspension {
                limit: EvaluationLimit::CaptureGroups,
                observed: 2,
                ..
            })
        ));
        assert!(matches!(
            evaluate_with_policy("[", &flags, "", &policy),
            EvaluationResult::InvalidPattern(_)
        ));
        assert!(matches!(
            evaluate_substitution_with_policy("(a)(b)", &flags, "ab", "x", &policy),
            SubstitutionEvaluationResult::Suspended(EvaluationSuspension {
                limit: EvaluationLimit::CaptureGroups,
                ..
            })
        ));
    }

    #[test]
    fn truncation_reports_lower_bounds_and_preserves_unicode_zero_width_rows() {
        let policy = EvaluationPolicy {
            stored_matches: 2,
            ..EvaluationPolicy::default()
        };
        let flags = RegexFlags::default();
        match evaluate_with_policy("", &flags, "éβ", &policy) {
            EvaluationResult::Success {
                matches,
                completeness,
            } => {
                assert_eq!(
                    completeness,
                    MatchCompleteness::Truncated {
                        reason: EvaluationLimit::StoredMatches,
                        at_least: 3
                    }
                );
                assert_eq!(matches.len(), 2);
                assert_eq!(matches[0].span, ByteSpan::new(0, 0).unwrap());
                assert_eq!(matches[1].span, ByteSpan::new(2, 2).unwrap());
                assert_eq!(matches[1].location.column, 2);
            }
            other => panic!("unexpected result: {other:?}"),
        }
        assert!(matches!(
            evaluate_with_policy("", &flags, "é", &policy),
            EvaluationResult::Success {
                completeness: MatchCompleteness::Complete,
                ..
            }
        ));
        let zero = EvaluationPolicy {
            stored_matches: 0,
            ..policy
        };
        assert!(matches!(
            evaluate_with_policy("", &flags, "", &zero),
            EvaluationResult::Success {
                completeness: MatchCompleteness::Truncated { at_least: 1, .. },
                ..
            }
        ));
    }

    #[test]
    fn materialization_budget_keeps_whole_capture_rows_and_accounts_for_names() {
        let flags = RegexFlags::default();
        let policy = EvaluationPolicy {
            materialized_bytes: 4,
            ..EvaluationPolicy::default()
        };
        match evaluate_with_policy("(é)(z)?", &flags, "éé", &policy) {
            EvaluationResult::Success {
                matches,
                completeness,
            } => {
                assert_eq!(
                    completeness,
                    MatchCompleteness::Truncated {
                        reason: EvaluationLimit::MaterializedBytes,
                        at_least: 2
                    }
                );
                assert_eq!(matches.len(), 1);
                assert_eq!(matches[0].captures.len(), 2);
                assert_eq!(matches[0].captures[1].value, CaptureValue::Unmatched);
                assert_eq!(matches[0].text, "é");
            }
            other => panic!("unexpected result: {other:?}"),
        }
        assert!(
            matches!(evaluate_with_policy("(?P<x>é)", &flags, "é", &policy), EvaluationResult::Success { matches, completeness: MatchCompleteness::Truncated { reason: EvaluationLimit::MaterializedBytes, at_least: 1 } } if matches.is_empty())
        );
        let zero = EvaluationPolicy {
            materialized_bytes: 0,
            ..policy
        };
        assert!(
            matches!(evaluate_with_policy("", &flags, "é", &zero), EvaluationResult::Success { matches, completeness: MatchCompleteness::Complete } if matches.len() == 2)
        );
    }

    #[test]
    fn substitution_checks_exact_expansion_size_before_copying_large_captures() {
        let flags = RegexFlags::default();
        let policy = EvaluationPolicy {
            replacement_output_bytes: 4,
            ..EvaluationPolicy::default()
        };
        assert_eq!(
            evaluate_substitution_with_policy("(é)", &flags, "é", "$1$1", &policy),
            SubstitutionEvaluationResult::Success(SubstitutionResult {
                output: "éé".into(),
                replacements_made: 1
            })
        );
        assert!(matches!(
            evaluate_substitution_with_policy("(é)", &flags, "é", "$1$1$1", &policy),
            SubstitutionEvaluationResult::Suspended(EvaluationSuspension {
                limit: EvaluationLimit::ReplacementOutputBytes,
                maximum: 4,
                observed: 6
            })
        ));
        let text = "x".repeat(super::super::policy::MAX_TEXT_BYTES);
        let replacement = "$1".repeat(1024);
        assert!(matches!(
            evaluate_substitution("(x+)", &flags, &text, &replacement),
            SubstitutionEvaluationResult::Suspended(EvaluationSuspension {
                limit: EvaluationLimit::ReplacementOutputBytes,
                observed: 67_108_864,
                ..
            })
        ));
    }

    #[test]
    fn substitution_never_returns_partial_success_after_any_output_or_count_limit() {
        let flags = RegexFlags::default();
        let policy = EvaluationPolicy {
            replacement_bytes: 3,
            replacement_output_bytes: 2,
            stored_matches: 1,
            ..EvaluationPolicy::default()
        };
        assert!(matches!(
            evaluate_substitution_with_policy("a", &flags, "a", "xxxx", &policy),
            SubstitutionEvaluationResult::Suspended(EvaluationSuspension {
                limit: EvaluationLimit::ReplacementBytes,
                ..
            })
        ));
        assert!(matches!(
            evaluate_substitution_with_policy("a", &flags, "a", "xxx", &policy),
            SubstitutionEvaluationResult::Suspended(EvaluationSuspension {
                limit: EvaluationLimit::ReplacementOutputBytes,
                ..
            })
        ));
        assert!(matches!(
            evaluate_substitution_with_policy("", &flags, "é", "", &policy),
            SubstitutionEvaluationResult::Suspended(EvaluationSuspension {
                limit: EvaluationLimit::StoredMatches,
                ..
            })
        ));
        assert!(matches!(
            evaluate_substitution_with_policy("z", &flags, "abc", "", &policy),
            SubstitutionEvaluationResult::Suspended(EvaluationSuspension {
                limit: EvaluationLimit::ReplacementOutputBytes,
                ..
            })
        ));
        assert!(matches!(
            evaluate_substitution_with_policy("z", &flags, "é", "", &policy),
            SubstitutionEvaluationResult::Success(SubstitutionResult {
                replacements_made: 0,
                ..
            })
        ));
    }

    #[test]
    fn bounded_substitution_preflight_agrees_with_rust_replacement_interpolation() {
        let flags = RegexFlags::default();
        let pattern = "(?P<first>é)(b)?";
        let text = "é éb";
        let regex = compile_regex(pattern, &flags).unwrap();
        for replacement in [
            "literal",
            "$0",
            "$1/$2",
            "$first",
            "${1}suffix",
            "$1suffix",
            "$$ $$1",
            "$missing ${missing}",
            "${first}/$2",
            "${unclosed",
            "$",
            r"\1",
        ] {
            assert_eq!(
                substitute(pattern, text, replacement).output,
                regex.replace_all(text, replacement),
                "{replacement}"
            );
        }
    }
}
