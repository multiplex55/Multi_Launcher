//! Source-bound sibling section moves, without Markdown reserialization.
use super::{MAX_MARKDOWN_SOURCE_BYTES, MarkdownEditError, PreparedMarkdownEdit, analyze_markdown};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionPlacement {
    Before,
    After,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnchorChange {
    pub heading: Range<usize>,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedSectionMove {
    pub edit: PreparedMarkdownEdit,
    pub moved_selection: Range<usize>,
    pub anchor_changes: Vec<AnchorChange>,
}

/// Heading indexes refer to the current complete analysis, not filtered outline rows.
pub fn prepare_section_move(
    source: &str,
    selected: usize,
    destination: usize,
    placement: SectionPlacement,
) -> Result<PreparedSectionMove, MarkdownEditError> {
    if source.len() > MAX_MARKDOWN_SOURCE_BYTES {
        return Err(MarkdownEditError::SourceTooLarge {
            max_bytes: MAX_MARKDOWN_SOURCE_BYTES,
        });
    }
    let h = super::headings::structural_headings(source);
    let visible: Vec<usize> = h
        .iter()
        .enumerate()
        .filter_map(|(index, heading)| (!heading.title.is_empty()).then_some(index))
        .collect();
    let selected = *visible
        .get(selected)
        .ok_or(MarkdownEditError::InvalidRange)?;
    let destination = *visible
        .get(destination)
        .ok_or(MarkdownEditError::InvalidRange)?;
    let (parents, ends) = super::sections::heading_hierarchy(&h);
    if selected == destination || parents[selected] != parents[destination] {
        return Err(MarkdownEditError::InvalidParameter(
            "Choose a different sibling section.",
        ));
    }
    let section_end = |i: usize| {
        h.get(ends[i])
            .map_or(source.len(), |next| next.byte_range.start)
    };
    let from = h[selected].byte_range.start;
    let end = section_end(selected);
    let target = match placement {
        SectionPlacement::Before => h[destination].byte_range.start,
        SectionPlacement::After => section_end(destination),
    };
    if target >= from && target <= end {
        return Err(MarkdownEditError::InvalidParameter(
            "This move has no effect or targets its own section.",
        ));
    }
    let range = from.min(target)..end.max(target);
    let moved = &source[from..end];
    let other = if target < from {
        &source[target..from]
    } else {
        &source[end..target]
    };
    let (first, second) = if target < from {
        (moved, other)
    } else {
        (other, moved)
    };
    let eol = match source.find('\n') {
        Some(i) if i > 0 && source.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    };
    let transfer = range.end == source.len() && !source.ends_with('\n');
    let (first, second, separator) = if transfer && !second.is_empty() {
        let trimmed = second.trim_end_matches(['\r', '\n']);
        (first, trimmed, &second[trimmed.len()..])
    } else {
        (first, second, "")
    };
    let mut replacement = String::new();
    replacement.push_str(first);
    if !separator.is_empty() {
        replacement.push_str(separator);
    } else if !first.is_empty() && !first.ends_with('\n') && !second.is_empty() {
        replacement.push_str(eol);
    }
    let seam = replacement.len();
    replacement.push_str(second);
    let mut selection_start = if target < from {
        range.start
    } else {
        range.start + seam
    };
    let mut selection_end = if target < from {
        range.start + moved.len()
    } else {
        range.start + replacement.len()
    };
    let mut edit = PreparedMarkdownEdit::prepare_with_replacement_limit(
        source,
        range.clone(),
        replacement,
        MAX_MARKDOWN_SOURCE_BYTES,
    )?;
    let mut candidate = edit.apply(source)?.source;
    if candidate == source {
        return Err(MarkdownEditError::InvalidParameter(
            "This move has no effect.",
        ));
    }
    let mut order: Vec<usize> = (0..h.len()).collect();
    let moving: Vec<usize> = order.drain(selected..ends[selected]).collect();
    let destination_index = order.iter().position(|&i| i == destination).unwrap();
    let insertion = match placement {
        SectionPlacement::Before => destination_index,
        SectionPlacement::After => order
            .iter()
            .position(|&i| i >= ends[destination])
            .unwrap_or(order.len()),
    };
    order.splice(insertion..insertion, moving);
    let mut after = super::headings::structural_headings(&candidate);
    if (after.len() != h.len()
        || after
            .iter()
            .zip(&order)
            .any(|(new, &old)| new.level != h[old].level || new.title != h[old].title))
        && !second.is_empty()
    {
        let mut with_blank = edit.replacement().to_owned();
        with_blank.insert_str(seam, eol);
        edit = PreparedMarkdownEdit::prepare_with_replacement_limit(
            source,
            range.clone(),
            with_blank,
            MAX_MARKDOWN_SOURCE_BYTES,
        )?;
        candidate = edit.apply(source)?.source;
        if target > from {
            selection_start += eol.len();
            selection_end += eol.len();
        }
        after = super::headings::structural_headings(&candidate);
    }
    if after.len() != h.len() {
        return Err(MarkdownEditError::InvalidParameter(
            "Move changes Markdown heading structure.",
        ));
    }
    let (new_parents, _) = super::sections::heading_hierarchy(&after);
    for (new_index, &old_index) in order.iter().enumerate() {
        let old = &h[old_index];
        let new = &after[new_index];
        if old.level != new.level
            || old.title != new.title
            || parents[old_index] != new_parents[new_index].map(|i| order[i])
        {
            return Err(MarkdownEditError::InvalidParameter(
                "Move changes Markdown heading structure.",
            ));
        }
    }
    let mut anchor_changes: Vec<AnchorChange> = order
        .iter()
        .enumerate()
        .filter_map(|(new_index, &old_index)| {
            let old = &h[old_index];
            let new = &after[new_index];
            (old.normalized_anchor != new.normalized_anchor).then(|| AnchorChange {
                heading: old.byte_range.clone(),
                before: old.normalized_anchor.clone(),
                after: new.normalized_anchor.clone(),
            })
        })
        .collect();
    let mut old_targets = std::collections::HashMap::new();
    let mut new_targets = std::collections::HashMap::new();
    for (index, heading) in h
        .iter()
        .enumerate()
        .filter(|(_, heading)| !heading.title.is_empty())
    {
        old_targets
            .entry(&heading.normalized_anchor)
            .or_insert(index);
    }
    for (index, heading) in after
        .iter()
        .enumerate()
        .filter(|(_, heading)| !heading.title.is_empty())
    {
        new_targets
            .entry(&heading.normalized_anchor)
            .or_insert(order[index]);
    }
    let mut warned: std::collections::HashSet<_> = anchor_changes
        .iter()
        .map(|change| change.heading.start)
        .collect();
    for (anchor, old_index) in old_targets {
        if new_targets
            .get(anchor)
            .is_some_and(|&new_index| new_index != old_index)
            && warned.insert(h[old_index].byte_range.start)
        {
            anchor_changes.push(AnchorChange {
                heading: h[old_index].byte_range.clone(),
                before: anchor.clone(),
                after: anchor.clone(),
            });
        }
    }
    Ok(PreparedSectionMove {
        edit,
        moved_selection: selection_start..selection_end,
        anchor_changes,
    })
}

/// Checks a proposed slot using the same source and hierarchy validation as preparation.
pub fn section_move_available(
    source: &str,
    selected: usize,
    destination: usize,
    placement: SectionPlacement,
) -> bool {
    prepare_section_move(source, selected, destination, placement).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moves_nested_sections_and_warns_on_duplicate_anchors() {
        let source = "preamble\n# Same\n## Child\ntext\n# Same\nend";
        let prepared = prepare_section_move(source, 0, 2, SectionPlacement::After).unwrap();
        assert_eq!(
            prepared.edit.apply(source).unwrap().source,
            "preamble\n# Same\nend\n# Same\n## Child\ntext"
        );
        assert_eq!(prepared.anchor_changes.len(), 2);
        assert_eq!(
            prepared.edit.apply("changed").unwrap_err(),
            MarkdownEditError::SourceChanged
        );
        assert!(prepare_section_move(source, 0, 1, SectionPlacement::After).is_err());
    }
    #[test]
    fn preserves_crlf_metadata_and_setext_ranges() {
        let source =
            "---\r\ntitle: X\r\n---\r\n前書き\r\n\r\nOne\r\n===\r\nbody\r\n\r\nTwo\r\n===\r\n終";
        let analysis = analyze_markdown(source);
        assert_eq!(analysis.headings.len(), 2);
        assert_eq!(
            &source[analysis.headings[0].byte_range.clone()],
            "One\r\n===\r\n"
        );
        let prepared = prepare_section_move(source, 1, 0, SectionPlacement::Before).unwrap();
        assert_eq!(
            prepared.edit.apply(source).unwrap().source,
            "---\r\ntitle: X\r\n---\r\n前書き\r\n\r\nTwo\r\n===\r\n終\r\n\r\nOne\r\n===\r\nbody"
        );
    }
    #[test]
    fn preserves_final_separator_and_indented_heading() {
        let source = "# A\nbody\n\n  # B\nlast";
        let prepared = prepare_section_move(source, 1, 0, SectionPlacement::Before).unwrap();
        assert_eq!(
            prepared.edit.apply(source).unwrap().source,
            "  # B\nlast\n\n# A\nbody"
        );
    }
    #[test]
    fn mixed_levels_and_noop_are_checked() {
        let source = "# Root\n### A\n### B\n## C\n";
        assert!(prepare_section_move(source, 1, 3, SectionPlacement::Before).is_ok());
        assert!(prepare_section_move(source, 1, 3, SectionPlacement::After).is_err());
        assert!(prepare_section_move("# X\n# X\n", 0, 1, SectionPlacement::After).is_err());
    }
    #[test]
    fn thematic_break_is_not_frontmatter() {
        let analysis = analyze_markdown("---\n\n# One\n\n---\n# Two\n");
        assert_eq!(analysis.headings.len(), 2);
    }

    #[test]
    fn adds_only_needed_setext_separator() {
        let source = "Title\n=====\nbody\n# A\nparagraph";
        let prepared = prepare_section_move(source, 1, 0, SectionPlacement::Before).unwrap();
        assert_eq!(
            prepared.edit.apply(source).unwrap().source,
            "# A\nparagraph\n\nTitle\n=====\nbody"
        );
    }

    #[test]
    fn setext_hash_title_and_anonymous_boundary() {
        let source = "#tag\n====\nbody\n# Next\nend";
        assert_eq!(analyze_markdown(source).headings[0].title, "#tag");
        assert!(prepare_section_move(source, 0, 1, SectionPlacement::After).is_ok());
        let source = "# A\nkeep\n#\nunnamed\n# B\nb";
        assert_eq!(analyze_markdown(source).headings.len(), 2);
        let moved = prepare_section_move(source, 0, 1, SectionPlacement::After)
            .unwrap()
            .edit
            .apply(source)
            .unwrap()
            .source;
        assert_eq!(moved, "#\nunnamed\n# B\nb\n# A\nkeep");
    }

    #[test]
    fn unicode_setext_and_comment_only_metadata() {
        let analysis = analyze_markdown("\u{00a0}# Label\n===\n");
        assert_eq!(analysis.headings[0].title, "# Label");
        let analysis = analyze_markdown("---\n# Heading: text\n---\n# Two\n");
        assert_eq!(analysis.headings.len(), 2);
    }
}
