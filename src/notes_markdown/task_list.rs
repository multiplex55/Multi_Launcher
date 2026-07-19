use std::ops::Range;

use super::{
    MarkdownTaskItem,
    parse::{LineSpan, leading_spaces, line_spans},
};

pub const TASK_LIST_LINE_RE: &str =
    r"^(\s*[-*]\s+\[( |x|X)\]\s*)(.*?)(\s*<!--\s*ml:todo:([A-Za-z0-9:_-]+)\s*-->\s*)?$";

pub fn parse_task_items(content: &str) -> Vec<MarkdownTaskItem> {
    let lines = line_spans(content);
    let code_lines = fenced_code_line_mask(&lines);
    parse_task_items_from_lines(&lines, &code_lines)
}

pub(crate) fn parse_task_items_from_lines(
    lines: &[LineSpan<'_>],
    code_lines: &[bool],
) -> Vec<MarkdownTaskItem> {
    lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| !code_lines.get(*idx).copied().unwrap_or(false))
        .filter_map(|(idx, line)| parse_task_item(idx, line))
        .collect()
}

pub fn toggle_task_marker(content: &str, marker_byte_range: Range<usize>) -> Option<String> {
    if marker_byte_range.start > marker_byte_range.end
        || marker_byte_range.end > content.len()
        || !content.is_char_boundary(marker_byte_range.start)
        || !content.is_char_boundary(marker_byte_range.end)
    {
        return None;
    }

    let replacement = match content.get(marker_byte_range.clone())? {
        "[ ]" => "[x]",
        "[x]" | "[X]" => "[ ]",
        _ => return None,
    };

    let mut updated = String::with_capacity(content.len());
    updated.push_str(&content[..marker_byte_range.start]);
    updated.push_str(replacement);
    updated.push_str(&content[marker_byte_range.end..]);
    Some(updated)
}

fn parse_task_item(line_index: usize, line: &LineSpan<'_>) -> Option<MarkdownTaskItem> {
    let indent = leading_spaces(line.text);
    let rest = &line.text[indent..];
    let bullet_marker = if rest.starts_with("- ") || rest.starts_with("* ") {
        rest[..1].to_string()
    } else {
        return None;
    };

    let after_bullet = &rest[2..];
    let marker = after_bullet.get(..3)?;
    let checked = match marker.as_bytes() {
        [b'[', b' ', b']'] => false,
        [b'[', b'x' | b'X', b']'] => true,
        _ => return None,
    };
    if after_bullet
        .as_bytes()
        .get(3)
        .is_some_and(|b| *b != b' ' && *b != b'\t')
    {
        return None;
    }
    let marker_start = line.start + indent + 2;
    Some(MarkdownTaskItem {
        line_index,
        line_byte_range: line.start..line.end,
        marker_byte_range: marker_start..marker_start + 3,
        checked,
        indent,
        bullet_marker,
        text: after_bullet[3..].trim_start().to_string(),
    })
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
                && marker == fence_marker
                && len >= fence_len
            {
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
    use super::*;

    #[test]
    fn parses_dash_and_star_checked_and_unchecked_markers() {
        let content = "- [ ] dash\n* [x] star\n- [X] upper\n";
        let items = parse_task_items(content);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].bullet_marker, "-");
        assert!(!items[0].checked);
        assert_eq!(items[1].bullet_marker, "*");
        assert!(items[1].checked);
        assert!(items[2].checked);
        assert_eq!(&content[items[0].marker_byte_range.clone()], "[ ]");
        assert_eq!(&content[items[1].marker_byte_range.clone()], "[x]");
        assert_eq!(&content[items[2].marker_byte_range.clone()], "[X]");
    }

    #[test]
    fn parses_nested_indentation_and_trailing_todo_metadata() {
        let content = "  - [ ] nested <!-- ml:todo:abc_123 -->\n    * [x] deeper\n";
        let items = parse_task_items(content);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].indent, 2);
        assert_eq!(items[0].text, "nested <!-- ml:todo:abc_123 -->");
        assert_eq!(items[1].indent, 4);
        assert_eq!(
            items[1].line_byte_range,
            content.find("    *").unwrap()..content.len()
        );
    }

    #[test]
    fn ignores_tasks_inside_fenced_code() {
        let content = "```\n- [x] ignored\n```\n- [ ] real\n";
        let items = parse_task_items(content);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "real");
    }

    #[test]
    fn unicode_text_preserves_byte_ranges_around_marker() {
        let content = "  * [X] Café 日記 🌱\n";
        let items = parse_task_items(content);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "Café 日記 🌱");
        assert_eq!(&content[items[0].line_byte_range.clone()], content);
        assert_eq!(&content[items[0].marker_byte_range.clone()], "[X]");
    }

    #[test]
    fn toggle_replaces_only_exact_marker_and_preserves_source() {
        let content = "before\n- [ ] task <!-- ml:todo:id -->\nafter\n";
        let item = parse_task_items(content).remove(0);
        let updated = toggle_task_marker(content, item.marker_byte_range).unwrap();
        assert_eq!(updated, "before\n- [x] task <!-- ml:todo:id -->\nafter\n");
    }

    #[test]
    fn toggle_validates_byte_boundaries_and_marker() {
        let content = "- [ ] café\n";
        assert_eq!(toggle_task_marker(content, 2..5).unwrap(), "- [x] café\n");
        assert!(toggle_task_marker(content, 0..1).is_none());
        assert!(toggle_task_marker(content, 2..content.len() + 1).is_none());
        let cafe = content.find('é').unwrap();
        assert!(toggle_task_marker(content, cafe..cafe + 1).is_none());
    }
}
