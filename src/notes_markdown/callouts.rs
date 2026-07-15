use super::{
    parse::{leading_spaces, line_spans, LineSpan},
    MarkdownCallout,
};

const KNOWN_CALLOUT_KINDS: &[&str] = &[
    "note",
    "tip",
    "important",
    "warning",
    "caution",
    "todo",
    "bug",
    "idea",
];

pub fn parse_callouts(content: &str) -> Vec<MarkdownCallout> {
    let lines = line_spans(content);
    let code_lines = fenced_code_line_mask(&lines);
    parse_callouts_from_lines(&lines, &code_lines)
}

pub(crate) fn parse_callouts_from_lines(
    lines: &[LineSpan<'_>],
    code_lines: &[bool],
) -> Vec<MarkdownCallout> {
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
    let raw_kind = rest[..close].trim();
    if raw_kind.is_empty() {
        return None;
    }
    let normalized = raw_kind.to_ascii_lowercase();
    let kind = if KNOWN_CALLOUT_KINDS.contains(&normalized.as_str()) {
        normalized
    } else {
        "note".to_string()
    };
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

fn fenced_code_line_mask(lines: &[LineSpan<'_>]) -> Vec<bool> {
    let mut mask = vec![false; lines.len()];
    let mut in_fence = false;
    let mut fence_marker = b'`';
    let mut fence_len = 0usize;

    for (idx, line) in lines.iter().enumerate() {
        let indent = leading_spaces(line.text);
        let trimmed = &line.text[indent..];
        let fence = fence_info(trimmed);
        if in_fence {
            mask[idx] = true;
            if let Some((marker, len)) = fence
                && marker == fence_marker && len >= fence_len {
                    in_fence = false;
                }
            continue;
        }

        if let Some((marker, len)) = fence.filter(|_| indent <= 3) {
            mask[idx] = true;
            in_fence = true;
            fence_marker = marker;
            fence_len = len;
        }
    }
    mask
}

fn fence_info(trimmed: &str) -> Option<(u8, usize)> {
    let bytes = trimmed.as_bytes();
    let marker = *bytes.first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let len = bytes.iter().take_while(|&&b| b == marker).count();
    (len >= 3).then_some((marker, len))
}

#[cfg(test)]
mod tests {
    use super::parse_callouts;

    #[test]
    fn parses_single_line_callout() {
        let callouts = parse_callouts("> [!NOTE]\n");

        assert_eq!(callouts.len(), 1);
        assert_eq!(callouts[0].kind, "note");
        assert_eq!(callouts[0].title, "");
        assert_eq!(callouts[0].body, "");
        assert_eq!(callouts[0].line_range, 0..1);
        assert_eq!(callouts[0].byte_range, 0..10);
    }

    #[test]
    fn parses_multi_line_body_with_markdown_stripped_from_blockquotes() {
        let content = "> [!TIP]\n> **bold**\n> - item\nnot part\n";
        let callouts = parse_callouts(content);

        assert_eq!(callouts.len(), 1);
        assert_eq!(callouts[0].kind, "tip");
        assert_eq!(callouts[0].body, "**bold**\n- item");
        assert_eq!(callouts[0].line_range, 0..3);
        assert_eq!(
            &content[callouts[0].byte_range.clone()],
            "> [!TIP]\n> **bold**\n> - item\n"
        );
    }

    #[test]
    fn parses_optional_title_after_marker() {
        let callouts = parse_callouts("> [!IMPORTANT] Read this\n> Body\n");

        assert_eq!(callouts.len(), 1);
        assert_eq!(callouts[0].kind, "important");
        assert_eq!(callouts[0].title, "Read this");
        assert_eq!(callouts[0].body, "Body");
    }

    #[test]
    fn falls_back_unknown_kind_to_note() {
        let callouts = parse_callouts("> [!CUSTOM] Something\n> Body\n");

        assert_eq!(callouts.len(), 1);
        assert_eq!(callouts[0].kind, "note");
        assert_eq!(callouts[0].title, "Something");
    }

    #[test]
    fn ignores_callouts_inside_fenced_code_blocks() {
        let content = "```\n> [!WARNING] ignored\n```\n> [!CAUTION] parsed\n";
        let callouts = parse_callouts(content);

        assert_eq!(callouts.len(), 1);
        assert_eq!(callouts[0].kind, "caution");
        assert_eq!(callouts[0].title, "parsed");
        assert_eq!(callouts[0].line_range, 3..4);
    }

    #[test]
    fn stores_correct_body_covering_line_and_byte_ranges() {
        let content = "Intro\n> [!TODO] Tasks\n> first\n> second\nAfter\n";
        let callouts = parse_callouts(content);

        assert_eq!(callouts.len(), 1);
        let callout = &callouts[0];
        assert_eq!(callout.body, "first\nsecond");
        assert_eq!(callout.line_range, 1..4);
        assert_eq!(
            &content[callout.byte_range.clone()],
            "> [!TODO] Tasks\n> first\n> second\n"
        );
    }

    #[test]
    fn parses_supported_kinds_case_insensitively() {
        let content = "> [!NoTe]\n> [!BUG]\n> [!idea]\n";
        let callouts = parse_callouts(content);

        assert_eq!(callouts.len(), 1);
        assert_eq!(callouts[0].kind, "note");
        assert_eq!(callouts[0].body, "[!BUG]\n[!idea]");
    }
}
