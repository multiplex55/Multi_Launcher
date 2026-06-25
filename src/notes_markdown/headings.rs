use std::collections::HashMap;

use super::{MarkdownHeading, parse::LineSpan};

pub(crate) fn parse_headings(lines: &[LineSpan<'_>], code_lines: &[bool]) -> Vec<MarkdownHeading> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes_markdown::{parse::line_spans, sections::parse_sections};

    fn parse(content: &str) -> Vec<MarkdownHeading> {
        let lines = line_spans(content);
        let code_lines = vec![false; lines.len()];
        parse_headings(&lines, &code_lines)
    }

    fn parse_with_code_mask(content: &str, code_lines: &[bool]) -> Vec<MarkdownHeading> {
        let lines = line_spans(content);
        parse_headings(&lines, code_lines)
    }

    #[test]
    fn extracts_atx_headings() {
        let content = "# One\nText\n### Three ###\n####### Nope\n  ## Indented\n";
        let headings = parse(content);

        assert_eq!(
            headings
                .iter()
                .map(|heading| (heading.level, heading.title.as_str(), heading.line_index))
                .collect::<Vec<_>>(),
            vec![(1, "One", 0), (3, "Three", 2), (2, "Indented", 4)]
        );
        assert_eq!(&content[headings[0].byte_range.clone()], "# One\n");
    }

    #[test]
    fn duplicate_anchors_get_incrementing_suffixes() {
        let headings = parse("# Repeat!\n## repeat\n# Repeat -- repeat?\n# Repeat\n");

        assert_eq!(
            headings
                .iter()
                .map(|heading| heading.normalized_anchor.as_str())
                .collect::<Vec<_>>(),
            vec!["repeat", "repeat-1", "repeat-repeat", "repeat-2"]
        );
    }

    #[test]
    fn sections_own_nested_content_until_same_or_higher_heading() {
        let content = "# A\na body\n## B\nb body\n### C\nc body\n## D\nd body\n# E\ne body\n";
        let headings = parse(content);
        let sections = parse_sections(&headings, content);

        assert_eq!(sections[0].heading.title, "A");
        assert_eq!(sections[0].body_line_range, 1..8);
        assert_eq!(sections[0].nested_heading_count, 3);
        assert_eq!(
            &content[sections[0].body_byte_range.clone()],
            "a body\n## B\nb body\n### C\nc body\n## D\nd body\n"
        );
        assert_eq!(sections[1].heading.title, "B");
        assert_eq!(sections[1].body_line_range, 3..6);
        assert_eq!(sections[1].nested_heading_count, 1);
        assert_eq!(sections[2].heading.title, "C");
        assert_eq!(sections[2].body_line_range, 5..6);
    }

    #[test]
    fn caller_can_filter_to_max_outline_depth() {
        let headings = parse("# One\n## Two\n### Three\n#### Four\n");
        let max_depth = 2;
        let outline = headings
            .iter()
            .filter(|heading| heading.level <= max_depth)
            .map(|heading| heading.title.as_str())
            .collect::<Vec<_>>();

        assert_eq!(outline, vec!["One", "Two"]);
        assert_eq!(headings.len(), 4);
    }

    #[test]
    fn fenced_code_lines_are_ignored_when_masked() {
        let content = "# Real\n```\n## Ignored\n```\n## Visible\n";
        let headings = parse_with_code_mask(content, &[false, true, true, true, false]);

        assert_eq!(
            headings
                .iter()
                .map(|heading| heading.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Real", "Visible"]
        );
    }

    #[test]
    fn unicode_headings_keep_unicode_anchor_text_and_byte_ranges() {
        let content = "# Café—日記  Привет №1!\n# 🌱\n";
        let headings = parse(content);

        assert_eq!(headings[0].title, "Café—日記  Привет №1!");
        assert_eq!(headings[0].normalized_anchor, "café日記-привет-1");
        assert_eq!(
            &content[headings[0].byte_range.clone()],
            "# Café—日記  Привет №1!\n"
        );
        assert_eq!(headings[1].normalized_anchor, "section");
    }

    #[test]
    fn notes_without_headings_return_empty_heading_list() {
        let headings = parse("plain text\n- [ ] task\n####### too many\n#\n");

        assert!(headings.is_empty());
    }
}
