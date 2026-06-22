use super::{MarkdownAnalysis, OutlineRow, callouts, headings, sections, task_list};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineSpan<'a> {
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
}

pub fn analyze_markdown(content: &str) -> MarkdownAnalysis {
    let lines = line_spans(content);
    let code_lines = code_line_mask(&lines);
    let headings = headings::parse_headings(&lines, &code_lines);
    let sections = sections::parse_sections(&lines, &headings);
    let task_items = task_list::parse_task_items(content);
    let callouts = callouts::parse_callouts(&lines, &code_lines);
    let outline = headings
        .iter()
        .map(|heading| OutlineRow {
            level: heading.level,
            title: heading.title.clone(),
            normalized_anchor: heading.normalized_anchor.clone(),
            line_index: heading.line_index,
        })
        .collect();

    MarkdownAnalysis {
        headings,
        sections,
        task_items,
        callouts,
        outline,
    }
}

pub(crate) fn line_spans(content: &str) -> Vec<LineSpan<'_>> {
    let mut spans = Vec::new();
    let mut start = 0;
    for line in content.split_inclusive('\n') {
        let end = start + line.len();
        let text = line.trim_end_matches(['\r', '\n']);
        spans.push(LineSpan { text, start, end });
        start = end;
    }
    if start < content.len() || content.is_empty() {
        if !content.is_empty() {
            spans.push(LineSpan {
                text: &content[start..],
                start,
                end: content.len(),
            });
        }
    }
    spans
}

pub(crate) fn leading_spaces(line: &str) -> usize {
    line.as_bytes().iter().take_while(|&&b| b == b' ').count()
}

fn code_line_mask(lines: &[LineSpan<'_>]) -> Vec<bool> {
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
            if let Some((marker, len)) = fence {
                if marker == fence_marker && len >= fence_len {
                    in_fence = false;
                }
            }
            continue;
        }

        if let Some((marker, len)) = fence.filter(|_| indent <= 3) {
            mask[idx] = true;
            in_fence = true;
            fence_marker = marker;
            fence_len = len;
        } else if indent >= 4 && !trimmed.is_empty() {
            mask[idx] = true;
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
    use super::analyze_markdown;

    #[test]
    fn empty_notes_have_no_markdown_items() {
        let analysis = analyze_markdown("");
        assert!(analysis.headings.is_empty());
        assert!(analysis.sections.is_empty());
        assert!(analysis.task_items.is_empty());
        assert!(analysis.callouts.is_empty());
        assert!(analysis.outline.is_empty());
    }

    #[test]
    fn unicode_content_keeps_byte_ranges_and_normalizes_anchors() {
        let content = "# Café 日記\n- [x] Привет мир\n> [!note] Täitle\n> body 🌱\n";
        let analysis = analyze_markdown(content);
        assert_eq!(analysis.headings[0].title, "Café 日記");
        assert_eq!(analysis.headings[0].normalized_anchor, "café-日記");
        assert_eq!(
            &content[analysis.headings[0].byte_range.clone()],
            "# Café 日記\n"
        );
        assert_eq!(analysis.task_items[0].text, "Привет мир");
        assert!(analysis.task_items[0].checked);
        assert_eq!(analysis.callouts[0].kind, "note");
        assert_eq!(analysis.callouts[0].body, "body 🌱");
    }

    #[test]
    fn mixed_line_endings_are_supported() {
        let content = "# One\r\ntext\n## Two\r\n- [ ] task\r\n";
        let analysis = analyze_markdown(content);
        assert_eq!(analysis.headings.len(), 2);
        assert_eq!(analysis.sections[0].body_line_range, 1..4);
        assert_eq!(analysis.task_items[0].line_index, 3);
        assert_eq!(
            &content[analysis.task_items[0].marker_byte_range.clone()],
            "[ ]"
        );
    }

    #[test]
    fn fenced_code_blocks_are_ignored() {
        let content =
            "# Real\n```\n# Not heading\n- [x] no task\n> [!warning] nope\n```\n- [ ] yes\n";
        let analysis = analyze_markdown(content);
        assert_eq!(analysis.headings.len(), 1);
        assert_eq!(analysis.task_items.len(), 1);
        assert_eq!(analysis.task_items[0].text, "yes");
        assert!(analysis.callouts.is_empty());
    }
}
