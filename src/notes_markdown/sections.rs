use super::{
    MarkdownHeading, MarkdownSection,
    parse::{LineSpan, line_spans},
};

pub fn parse_sections(headings: &[MarkdownHeading], content: &str) -> Vec<MarkdownSection> {
    let lines = line_spans(content);
    parse_sections_from_lines(&lines, headings)
}

fn parse_sections_from_lines(
    lines: &[LineSpan<'_>],
    headings: &[MarkdownHeading],
) -> Vec<MarkdownSection> {
    let (_, ends) = heading_hierarchy(headings);
    headings
        .iter()
        .enumerate()
        .map(|(idx, heading)| {
            let end_idx = ends[idx];
            let end_line = headings
                .get(end_idx)
                .map_or(lines.len(), |next| next.line_index);
            let body_start_byte = heading.byte_range.end;
            let body_start_line = lines.partition_point(|line| line.end <= body_start_byte);
            let body_end_byte = headings.get(end_idx).map_or_else(
                || lines.last().map_or(0, |line| line.end),
                |next| next.byte_range.start,
            );
            MarkdownSection {
                heading: heading.clone(),
                body_line_range: body_start_line..end_line,
                body_byte_range: body_start_byte..body_end_byte,
                nested_heading_count: end_idx - idx - 1,
            }
        })
        .collect()
}

pub(crate) fn heading_hierarchy(headings: &[MarkdownHeading]) -> (Vec<Option<usize>>, Vec<usize>) {
    let mut parents = vec![None; headings.len()];
    let mut ends = vec![headings.len(); headings.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (index, heading) in headings.iter().enumerate() {
        while stack
            .last()
            .is_some_and(|&old| headings[old].level >= heading.level)
        {
            ends[stack.pop().unwrap()] = index;
        }
        parents[index] = stack.last().copied();
        stack.push(index);
    }
    (parents, ends)
}
