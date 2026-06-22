use super::{MarkdownHeading, MarkdownSection, parse::LineSpan};

pub fn parse_sections(
    lines: &[LineSpan<'_>],
    headings: &[MarkdownHeading],
) -> Vec<MarkdownSection> {
    headings
        .iter()
        .enumerate()
        .map(|(idx, heading)| {
            let end_heading_idx = headings
                .iter()
                .enumerate()
                .skip(idx + 1)
                .find(|(_, candidate)| candidate.level <= heading.level)
                .map(|(next_idx, _)| next_idx)
                .unwrap_or(headings.len());
            let end_line = headings
                .get(end_heading_idx)
                .map(|h| h.line_index)
                .unwrap_or(lines.len());
            let body_start_line = heading.line_index + 1;
            let body_start_byte = lines
                .get(body_start_line)
                .map(|l| l.start)
                .unwrap_or(heading.byte_range.end);
            let body_end_byte = lines
                .get(end_line)
                .map(|l| l.start)
                .unwrap_or_else(|| lines.last().map(|l| l.end).unwrap_or(0));
            MarkdownSection {
                heading: heading.clone(),
                body_line_range: body_start_line..end_line,
                body_byte_range: body_start_byte..body_end_byte,
                nested_heading_count: end_heading_idx.saturating_sub(idx + 1),
            }
        })
        .collect()
}
