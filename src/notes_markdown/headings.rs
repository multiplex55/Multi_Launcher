use std::collections::HashMap;

use super::{parse::LineSpan, MarkdownHeading};

pub fn parse_headings(lines: &[LineSpan<'_>], code_lines: &[bool]) -> Vec<MarkdownHeading> {
    let mut seen_anchors = HashMap::new();

    lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| !code_lines.get(*idx).copied().unwrap_or(false))
        .filter_map(|(idx, line)| parse_heading(idx, line))
        .map(|mut heading| {
            heading.normalized_anchor =
                unique_anchor(&heading.normalized_anchor, &mut seen_anchors);
            heading
        })
        .collect()
}

fn parse_heading(line_index: usize, line: &LineSpan<'_>) -> Option<MarkdownHeading> {
    let indent = line
        .text
        .as_bytes()
        .iter()
        .take_while(|&&b| b == b' ')
        .count();
    if indent > 3 {
        return None;
    }
    let rest = &line.text[indent..];
    let level = rest.as_bytes().iter().take_while(|&&b| b == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    if rest
        .as_bytes()
        .get(level)
        .is_some_and(|b| *b != b' ' && *b != b'\t')
    {
        return None;
    }

    let title = strip_closing_sequence(rest[level..].trim()).to_string();
    if title.is_empty() {
        return None;
    }
    Some(MarkdownHeading {
        level: level as u8,
        normalized_anchor: normalize_anchor(&title),
        title,
        line_index,
        byte_range: line.start..line.end,
    })
}

fn strip_closing_sequence(raw: &str) -> &str {
    let trimmed = raw.trim_end();
    let Some(hash_start) = trimmed.rfind(|ch| ch != '#') else {
        return "";
    };
    let closing_start = hash_start + trimmed[hash_start..].chars().next().unwrap().len_utf8();
    let closing = &trimmed[closing_start..];
    if closing.is_empty() {
        return trimmed;
    }
    let before_closing = &trimmed[..closing_start];
    if before_closing.ends_with(char::is_whitespace) {
        before_closing.trim_end()
    } else {
        trimmed
    }
}

/// Normalizes a heading title into a deterministic, slug-like anchor.
///
/// The normalization lowercases Unicode, trims/collapses whitespace and dashes into single `-`
/// separators, removes punctuation/symbols, and keeps Unicode letters and numbers intact.
pub fn normalize_anchor(title: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;

    for ch in title.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if (ch.is_whitespace() || ch == '-') && !out.is_empty() && !last_dash {
            out.push('-');
            last_dash = true;
        }
    }

    if last_dash {
        out.pop();
    }
    if out.is_empty() {
        "section".to_string()
    } else {
        out
    }
}

fn unique_anchor(anchor: &str, seen_anchors: &mut HashMap<String, usize>) -> String {
    let count = seen_anchors.entry(anchor.to_string()).or_insert(0);
    let unique = if *count == 0 {
        anchor.to_string()
    } else {
        format!("{anchor}-{count}")
    };
    *count += 1;
    unique
}
