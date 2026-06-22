use super::{MarkdownHeading, parse::LineSpan};

pub fn parse_headings(lines: &[LineSpan<'_>], code_lines: &[bool]) -> Vec<MarkdownHeading> {
    lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| !code_lines.get(*idx).copied().unwrap_or(false))
        .filter_map(|(idx, line)| parse_heading(idx, line))
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
    let raw = rest[level..].trim();
    let title = raw.trim_end_matches('#').trim_end().to_string();
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

pub fn normalize_anchor(title: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in title.chars().flat_map(char::to_lowercase) {
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
    out
}
