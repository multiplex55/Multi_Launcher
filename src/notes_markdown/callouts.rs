use super::{
    MarkdownCallout,
    parse::{LineSpan, leading_spaces},
};

pub fn parse_callouts(lines: &[LineSpan<'_>], code_lines: &[bool]) -> Vec<MarkdownCallout> {
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < lines.len() {
        if code_lines.get(idx).copied().unwrap_or(false) {
            idx += 1;
            continue;
        }
        if let Some((kind, title)) = callout_header(lines[idx].text) {
            let start = idx;
            idx += 1;
            let mut body = Vec::new();
            while idx < lines.len() && !code_lines.get(idx).copied().unwrap_or(false) {
                if let Some(stripped) = blockquote_text(lines[idx].text) {
                    body.push(stripped.to_string());
                    idx += 1;
                } else {
                    break;
                }
            }
            let end = idx;
            out.push(MarkdownCallout {
                kind,
                title,
                line_range: start..end,
                byte_range: lines[start].start..lines[end.saturating_sub(1)].end,
                body: body.join("\n"),
            });
        } else {
            idx += 1;
        }
    }
    out
}

fn callout_header(line: &str) -> Option<(String, String)> {
    let text = blockquote_text(line)?;
    let rest = text.strip_prefix("[!")?;
    let close = rest.find(']')?;
    let kind = rest[..close].trim().to_ascii_lowercase();
    if kind.is_empty() {
        return None;
    }
    Some((kind, rest[close + 1..].trim().to_string()))
}

fn blockquote_text(line: &str) -> Option<&str> {
    let indent = leading_spaces(line);
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let rest = rest.strip_prefix('>')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}
