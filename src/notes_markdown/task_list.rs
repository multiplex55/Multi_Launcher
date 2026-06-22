use super::{
    MarkdownTaskItem,
    parse::{LineSpan, leading_spaces},
};

pub fn parse_task_items(lines: &[LineSpan<'_>], code_lines: &[bool]) -> Vec<MarkdownTaskItem> {
    lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| !code_lines.get(*idx).copied().unwrap_or(false))
        .filter_map(|(idx, line)| parse_task_item(idx, line))
        .collect()
}

fn parse_task_item(line_index: usize, line: &LineSpan<'_>) -> Option<MarkdownTaskItem> {
    let indent = leading_spaces(line.text);
    let rest = &line.text[indent..];
    let (bullet_len, bullet_marker) =
        if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
            (1, rest[..1].to_string())
        } else {
            let dot = rest.find(['.', ')'])?;
            if dot == 0
                || !rest[..dot].chars().all(|c| c.is_ascii_digit())
                || rest.as_bytes().get(dot + 1) != Some(&b' ')
            {
                return None;
            }
            (dot + 1, rest[..=dot].to_string())
        };
    let after_bullet = &rest[bullet_len + 1..];
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
    let marker_start = line.start + indent + bullet_len + 1;
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
