//! Renderer-neutral formatting for explicit Regex Tester clipboard actions.
use super::{CaptureValue, RegexMatch, RegexTestCaseSummary};
use std::fmt::Write;

pub fn all_matches(matches: &[RegexMatch]) -> String {
    matches.iter().map(|item| item.text.as_str()).collect::<Vec<_>>().join("\n")
}

pub fn match_information(matched: &RegexMatch) -> String {
    let bytes = |span: super::ByteSpan| format!("UTF-8 bytes {}..{} (end exclusive)", span.start_byte(), span.end_byte());
    let mut text = format!("Match #{}\nLine {}, column {} (Unicode scalars)\n{}\nText{}:\n{}\n", matched.id.index() + 1, matched.location.line, matched.location.column, bytes(matched.span), if matched.text.is_empty() { " (zero-width match, empty)" } else { "" }, matched.text);
    for capture in &matched.captures {
        let _ = write!(text, "Capture #{}", capture.group_index);
        if let Some(name) = &capture.name { let _ = write!(text, " ({name})"); }
        text.push('\n');
        match &capture.value {
            CaptureValue::Unmatched => text.push_str("Unmatched optional capture\n"),
            CaptureValue::Matched { span, text: value } => {
                let _ = writeln!(text, "{}", bytes(*span));
                if value.is_empty() { text.push_str("Matched empty capture\n"); }
                else { let _ = writeln!(text, "Matched text:\n{value}"); }
            }
        }
    }
    text
}

pub fn case_summary(summary: RegexTestCaseSummary) -> String {
    let noun = if summary.total == 1 { "case" } else { "cases" };
    format!("{} {noun} · {} passed · {} failed · {} errors · {} pending", summary.total, summary.passed, summary.failed, summary.pattern_errors + summary.unevaluated, summary.pending)
}
