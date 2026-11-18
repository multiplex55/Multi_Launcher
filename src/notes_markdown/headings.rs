use std::collections::HashMap;

use super::{MarkdownHeading, parse::LineSpan};

pub(crate) fn parse_headings(content: &str, lines: &[LineSpan<'_>]) -> Vec<MarkdownHeading> {
    parse_heading_events(content, lines, false)
}

pub(crate) fn structural_headings(content: &str) -> Vec<MarkdownHeading> {
    let lines = super::parse::line_spans(content);
    parse_heading_events(content, &lines, true)
}

fn parse_heading_events(
    content: &str,
    lines: &[LineSpan<'_>],
    include_empty: bool,
) -> Vec<MarkdownHeading> {
    use pulldown_cmark::{Event, Options, Parser, Tag};
    let start = initial_metadata_end(content);
    let mut seen_anchors = HashMap::new();
    let mut headings = Vec::new();
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(
        &content[start..],
        Options::ENABLE_TABLES
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_FOOTNOTES,
    )
    .into_offset_iter()
    {
        match event {
            Event::Start(tag) => {
                if let Tag::Heading { level, .. } = tag
                    && depth == 0
                {
                    let parsed_begin = start + range.start;
                    let line_index = lines.partition_point(|line| line.end <= parsed_begin);
                    if let Some(line) = lines.get(line_index) {
                        let begin = line.start;
                        let mut end = start + range.end;
                        if end > begin && content.as_bytes().get(end - 1) != Some(&b'\n') {
                            end = content[end..]
                                .find('\n')
                                .map_or(content.len(), |n| end + n + 1);
                        }
                        let level = level as u8;
                        // The parser's heading event spans the underline for setext.
                        let atx = start + range.end <= line.end;
                        let title = if atx {
                            parse_heading(line_index, line)
                                .map(|heading| heading.title)
                                .unwrap_or_default()
                        } else {
                            let raw = &content[begin..end];
                            let mut rows: Vec<&str> = raw.lines().collect();
                            rows.pop(); // The parser authenticated this last line as the underline.
                            rows.into_iter()
                                .map(str::trim)
                                .collect::<Vec<_>>()
                                .join(" ")
                        };
                        if include_empty || !title.is_empty() {
                            let anchor = normalize_anchor(&title);
                            let normalized_anchor = if title.is_empty() {
                                String::new()
                            } else {
                                unique_anchor(&anchor, &mut seen_anchors)
                            };
                            headings.push(MarkdownHeading {
                                level,
                                title,
                                normalized_anchor,
                                line_index,
                                byte_range: begin..end,
                            });
                        }
                    }
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    headings
}

fn initial_metadata_end(content: &str) -> usize {
    let mut lines = content.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return 0;
    };
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return 0;
    }
    let mut end = first.len();
    let mut mapping = false;
    for line in lines {
        end += line.len();
        let text = line.trim_end_matches(['\r', '\n']);
        if text == "---" {
            return if mapping { end } else { 0 };
        }
        if !text.starts_with('#')
            && !text.starts_with(char::is_whitespace)
            && text
                .split_once(':')
                .is_some_and(|(key, _)| !key.trim().is_empty())
        {
            mapping = true;
        }
        if text.trim().is_empty() && !mapping {
            return 0;
        }
    }
    0
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
        parse_headings(content, &lines)
    }

    fn parse_with_code_mask(content: &str, code_lines: &[bool]) -> Vec<MarkdownHeading> {
        let lines = line_spans(content);
        let _ = code_lines;
        parse_headings(content, &lines)
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
