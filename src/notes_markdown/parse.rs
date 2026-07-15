use super::{callouts, headings, sections, task_list, MarkdownAnalysis, OutlineRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineSpan<'a> {
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
}

pub fn analyze_markdown(content: &str) -> MarkdownAnalysis {
    analyze_markdown_with_max_outline_depth(content, 6)
}

pub fn analyze_markdown_with_max_outline_depth(
    content: &str,
    max_outline_depth: usize,
) -> MarkdownAnalysis {
    let lines = line_spans(content);
    let code_lines = code_line_mask(&lines);
    let headings = headings::parse_headings(&lines, &code_lines);
    let sections = sections::parse_sections(&headings, content);
    let task_items = task_list::parse_task_items(content);
    let callouts = callouts::parse_callouts(content);
    let max_outline_depth = max_outline_depth.clamp(1, 6) as u8;
    let outline = headings
        .iter()
        .filter(|heading| heading.level <= max_outline_depth)
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
    if (start < content.len() || content.is_empty())
        && !content.is_empty() {
            spans.push(LineSpan {
                text: &content[start..],
                start,
                end: content.len(),
            });
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
    use super::{analyze_markdown, analyze_markdown_with_max_outline_depth};

    #[test]
    fn extracts_atx_headings_and_ignores_non_headings() {
        let content = "# One\n####### Not heading\n### Three ###\n    # Code block\n###### Six\n";
        let analysis = analyze_markdown(content);
        let headings: Vec<_> = analysis
            .headings
            .iter()
            .map(|heading| (heading.level, heading.title.as_str(), heading.line_index))
            .collect();

        assert_eq!(
            headings,
            vec![(1, "One", 0), (3, "Three", 2), (6, "Six", 4)]
        );
        assert_eq!(analysis.headings[1].normalized_anchor, "three");
    }

    #[test]
    fn duplicate_heading_anchors_get_stable_suffixes() {
        let content = "# Repeat!\n## repeat\n# Repeat -- repeat?\n# Repeat\n";
        let analysis = analyze_markdown(content);
        let anchors: Vec<_> = analysis
            .headings
            .iter()
            .map(|heading| heading.normalized_anchor.as_str())
            .collect();

        assert_eq!(
            anchors,
            vec!["repeat", "repeat-1", "repeat-repeat", "repeat-2"]
        );
        assert_eq!(
            analysis
                .outline
                .iter()
                .map(|row| row.normalized_anchor.as_str())
                .collect::<Vec<_>>(),
            anchors
        );
    }

    #[test]
    fn nested_sections_are_owned_until_same_or_higher_heading() {
        let content = "# A\na body\n## B\nb body\n### C\nc body\n## D\nd body\n# E\ne body\n";
        let analysis = analyze_markdown(content);

        assert_eq!(analysis.sections[0].heading.title, "A");
        assert_eq!(analysis.sections[0].body_line_range, 1..8);
        assert_eq!(analysis.sections[0].nested_heading_count, 3);
        assert_eq!(
            &content[analysis.sections[0].body_byte_range.clone()],
            "a body\n## B\nb body\n### C\nc body\n## D\nd body\n"
        );

        assert_eq!(analysis.sections[1].heading.title, "B");
        assert_eq!(analysis.sections[1].body_line_range, 3..6);
        assert_eq!(analysis.sections[1].nested_heading_count, 1);

        assert_eq!(analysis.sections[2].heading.title, "C");
        assert_eq!(analysis.sections[2].body_line_range, 5..6);
        assert_eq!(analysis.sections[2].nested_heading_count, 0);
    }

    #[test]
    fn fenced_code_headings_are_ignored() {
        let content = "# Real\n~~~md\n## Ignored\n~~~\n```\n### Also ignored\n```\n## Visible\n";
        let analysis = analyze_markdown(content);
        assert_eq!(
            analysis
                .headings
                .iter()
                .map(|heading| heading.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Real", "Visible"]
        );
    }

    #[test]
    fn unicode_heading_anchors_keep_letters_and_numbers() {
        let content = "# Café—日記  Привет №1!\n# 🌱\n";
        let analysis = analyze_markdown(content);
        assert_eq!(analysis.headings[0].normalized_anchor, "café日記-привет-1");
        assert_eq!(analysis.headings[1].normalized_anchor, "section");
    }

    #[test]
    fn notes_without_headings_have_no_sections_or_outline() {
        let analysis = analyze_markdown("plain text\n- [ ] task\n");
        assert!(analysis.headings.is_empty());
        assert!(analysis.sections.is_empty());
        assert!(analysis.outline.is_empty());
    }

    #[test]
    fn outline_respects_max_depth_filter() {
        let analysis = analyze_markdown_with_max_outline_depth("# One\n## Two\n### Three\n", 2);
        assert_eq!(
            analysis
                .outline
                .iter()
                .map(|row| (row.level, row.title.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, "One"), (2, "Two")]
        );
        assert_eq!(analysis.headings.len(), 3);
    }

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
