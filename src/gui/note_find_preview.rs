//! Semantic source mapping for Find navigation in the note preview.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Debug, Default)]
pub(super) struct NoteFindPreviewIndex {
    pub(super) blocks: Vec<NoteFindPreviewBlock>,
    reference_definitions: String,
    reference_definition_ranges: Vec<Range<usize>>,
    has_footnotes: bool,
    todo_tokens: Vec<TodoToken>,
    wiki_tokens: Vec<WikiToken>,
    images: Vec<ImageSource>,
}

#[derive(Clone, Debug)]
pub(super) struct NoteFindPreviewBlock {
    pub(super) source_range: Range<usize>,
    pub(super) visible_ranges: Vec<Range<usize>>,
}

#[derive(Clone, Debug)]
struct TodoToken {
    token: Range<usize>,
    id_range: Range<usize>,
    id: String,
}

#[derive(Clone, Debug)]
struct WikiToken {
    token: Range<usize>,
    target: String,
}

#[derive(Clone, Debug)]
struct ImageSource {
    token: Range<usize>,
    alt: Range<usize>,
    path: Range<usize>,
}

impl NoteFindPreviewIndex {
    pub(super) fn new(source: &str) -> Self {
        let parser = Parser::new_ext(source, preview_parser_options());
        let mut reference_definitions = String::new();
        let mut reference_definition_ranges = Vec::new();
        for (_, definition_info) in parser.reference_definitions().iter() {
            if let Some(definition) = source.get(definition_info.span.clone()) {
                reference_definition_ranges.push(definition_info.span.clone());
                reference_definitions.push_str(definition);
                reference_definitions.push('\n');
            }
        }

        let mut blocks = Vec::new();
        let mut block_start = None;
        let mut block_depth = 0usize;
        let mut visible_ranges = Vec::new();
        let mut image_depth = 0usize;
        let mut code_ranges = Vec::new();
        let mut code_block_starts = Vec::new();
        let mut footnote_depth = 0usize;
        let mut has_footnotes = false;
        let mut footnote_definition_starts = Vec::new();
        let mut footnote_definitions = Vec::<(String, Range<usize>)>::new();
        let mut footnote_references = Vec::<(String, Range<usize>)>::new();

        for (event, range) in parser.into_offset_iter() {
            match &event {
                Event::Start(Tag::Image { .. }) => image_depth += 1,
                Event::End(TagEnd::Image) => image_depth = image_depth.saturating_sub(1),
                Event::Start(Tag::CodeBlock(_)) => code_block_starts.push(range.start),
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(start) = code_block_starts.pop() {
                        code_ranges.push(start..range.end);
                    }
                }
                Event::Code(_) => code_ranges.push(range.clone()),
                Event::Start(Tag::FootnoteDefinition(label)) => {
                    has_footnotes = true;
                    footnote_depth += 1;
                    footnote_definition_starts.push((label.to_string(), range.start));
                }
                Event::End(TagEnd::FootnoteDefinition) => {
                    footnote_depth = footnote_depth.saturating_sub(1);
                    if let Some((label, start)) = footnote_definition_starts.pop() {
                        footnote_definitions.push((label, start..range.end));
                    }
                }
                Event::FootnoteReference(label) => {
                    footnote_references.push((label.to_string(), range.clone()));
                }
                _ => {}
            }

            match &event {
                Event::Start(_) => {
                    if block_depth == 0 {
                        block_start = Some(range.start);
                        visible_ranges.clear();
                    }
                    block_depth += 1;
                }
                Event::End(_) => {
                    block_depth = block_depth.saturating_sub(1);
                    if block_depth == 0
                        && let Some(start) = block_start.take()
                    {
                        blocks.push(NoteFindPreviewBlock {
                            source_range: start..range.end,
                            visible_ranges: std::mem::take(&mut visible_ranges),
                        });
                    }
                }
                Event::Rule | Event::Html(_) if block_depth == 0 => {
                    blocks.push(NoteFindPreviewBlock {
                        source_range: range.clone(),
                        visible_ranges: event_visible_ranges(source, &event, &range, image_depth),
                    });
                }
                _ if block_depth > 0 => {
                    visible_ranges.extend(event_visible_ranges(
                        source,
                        &event,
                        &range,
                        image_depth,
                    ));
                }
                _ => {}
            }

            if footnote_depth > 0 {
                has_footnotes = true;
            }
        }

        if has_footnotes {
            coalesce_footnote_blocks(&mut blocks, &footnote_references, &footnote_definitions);
        }

        let todo_tokens = TODO_TOKEN_RE
            .captures_iter(source)
            .filter_map(|captures| {
                let whole = captures.get(0)?;
                let id = captures.get(1)?;
                let token = whole.start()..whole.end();
                (!code_ranges.iter().any(|code| ranges_overlap(code, &token))).then(|| TodoToken {
                    token,
                    id_range: id.start()..id.end(),
                    id: id.as_str().to_owned(),
                })
            })
            .collect();
        let wiki_tokens = WIKI_TOKEN_RE
            .find_iter(source)
            .filter_map(|found| {
                let token = found.start()..found.end();
                if code_ranges.iter().any(|code| ranges_overlap(code, &token)) {
                    return None;
                }
                let inner = &source[found.start() + 2..found.end() - 2];
                Some(WikiToken {
                    token,
                    target: inner.split('|').next().unwrap_or(inner).trim().to_owned(),
                })
            })
            .collect();
        let images = IMAGE_SOURCE_RE
            .captures_iter(source)
            .filter_map(|captures| {
                let whole = captures.get(0)?;
                let alt = captures.get(1)?;
                let target = captures.get(2)?;
                let path = target.as_str().split('|').next().unwrap_or(target.as_str());
                let leading = path.len() - path.trim_start().len();
                let trailing = path.trim_end().len();
                Some(ImageSource {
                    token: whole.start()..whole.end(),
                    alt: alt.start()..alt.end(),
                    path: (target.start() + leading)..(target.start() + trailing),
                })
            })
            .collect();

        Self {
            blocks,
            reference_definitions,
            reference_definition_ranges,
            has_footnotes,
            todo_tokens,
            wiki_tokens,
            images,
        }
    }

    pub(super) fn rendered_block(&self, source: &str, block: &NoteFindPreviewBlock) -> String {
        let mut rendered = source[block.source_range.clone()].to_owned();
        if !self.reference_definitions.is_empty() {
            rendered.push_str("\n\n");
            rendered.push_str(&self.reference_definitions);
        }
        rendered
    }

    pub(super) fn block_for_partial_source_range(
        &self,
        source: &str,
        source_range: Range<usize>,
    ) -> NoteFindPreviewBlock {
        let visible_ranges = self
            .blocks
            .iter()
            .flat_map(|block| block.visible_ranges.iter())
            .filter_map(|visible| {
                let start = visible.start.max(source_range.start);
                let end = visible.end.min(source_range.end);
                (start < end && source.is_char_boundary(start) && source.is_char_boundary(end))
                    .then_some(start..end)
            })
            .collect();
        NoteFindPreviewBlock {
            source_range,
            visible_ranges,
        }
    }

    pub(super) fn renderable_uncovered_ranges(
        &self,
        source: &str,
        source_range: Range<usize>,
    ) -> Vec<Range<usize>> {
        let mut ranges = vec![source_range];
        for definition in &self.reference_definition_ranges {
            subtract_range(&mut ranges, definition);
        }
        ranges
            .into_iter()
            .filter(|range| {
                source
                    .get(range.clone())
                    .is_some_and(|text| !text.trim().is_empty())
            })
            .collect()
    }

    pub(super) fn has_footnotes(&self) -> bool {
        self.has_footnotes
    }

    pub(super) fn visible_ranges_for_block(
        &self,
        source: &str,
        block: &NoteFindPreviewBlock,
        current_slug: &str,
        todo_labels: &std::collections::HashMap<String, String>,
        images_as_links: bool,
    ) -> Vec<Range<usize>> {
        let mut visible = block.visible_ranges.clone();

        // Todo links are replaced before CommonMark sees them. Only the ID
        // fallback has a source range that is also the rendered label; a
        // derived human label has no matching source text to map honestly.
        for todo in self.todo_tokens.iter().filter(|token| {
            block.source_range.start <= token.token.start
                && token.token.end <= block.source_range.end
        }) {
            subtract_range(&mut visible, &todo.token);
            if todo_labels
                .get(&todo.id)
                .is_none_or(|label| label == &todo.id)
            {
                visible.push(todo.id_range.clone());
            }
        }

        // Normal Markdown preprocessing keeps the entire wiki label visible
        // (including target|alias); only the delimiters are source syntax.
        // Self-links bypass preprocessing and remain literal source text.
        for wiki in self.wiki_tokens.iter().filter(|token| {
            block.source_range.start <= token.token.start
                && token.token.end <= block.source_range.end
        }) {
            if crate::common::slug::slugify(&wiki.target) == current_slug {
                continue;
            }
            subtract_range(&mut visible, &wiki.token);
            visible.push((wiki.token.start + 2)..(wiki.token.end - 2));
        }

        // Custom image rendering exposes alt text or, when alt is empty, its
        // path only in "display images as links" mode. The image widget itself
        // has no source text to locate.
        for image in self.images.iter().filter(|image| {
            block.source_range.start <= image.token.start
                && image.token.end <= block.source_range.end
        }) {
            subtract_range(&mut visible, &image.token);
            if images_as_links {
                let range = if image.alt.is_empty() {
                    image.path.clone()
                } else {
                    image.alt.clone()
                };
                if range.start < range.end {
                    visible.push(range);
                }
            }
        }

        visible.retain(|range| {
            range.start >= block.source_range.start
                && range.end <= block.source_range.end
                && source.is_char_boundary(range.start)
                && source.is_char_boundary(range.end)
        });
        visible.sort_by_key(|range| (range.start, range.end));
        visible
    }

    pub(super) fn match_is_visible_in(
        &self,
        source: &str,
        match_range: &Range<usize>,
        current_slug: &str,
        todo_labels: &std::collections::HashMap<String, String>,
        images_as_links: bool,
    ) -> bool {
        self.blocks.iter().any(|block| {
            block.source_range.start <= match_range.start
                && match_range.end <= block.source_range.end
                && self
                    .visible_ranges_for_block(
                        source,
                        block,
                        current_slug,
                        todo_labels,
                        images_as_links,
                    )
                    .iter()
                    .any(|visible| {
                        visible.start <= match_range.start && match_range.end <= visible.end
                    })
        })
    }

    pub(super) fn visible_ranges_for_image(
        &self,
        image_range: &Range<usize>,
        images_as_links: bool,
    ) -> Vec<Range<usize>> {
        if !images_as_links {
            return Vec::new();
        }
        self.images
            .iter()
            .filter(|image| image.token == *image_range)
            .map(|image| {
                if image.alt.is_empty() {
                    image.path.clone()
                } else {
                    image.alt.clone()
                }
            })
            .collect()
    }

    pub(super) fn block_for_match(
        &self,
        match_range: &Range<usize>,
    ) -> Option<&NoteFindPreviewBlock> {
        self.blocks.iter().find(|block| {
            block.source_range.start <= match_range.start
                && match_range.end <= block.source_range.end
        })
    }
}

/// Returns exact source spans that `render_inline_note_text` displays as text
/// for a task-row label. Wiki aliases and ordinary web-link destinations are
/// hidden by that custom renderer; todo tokens are displayed in full.
pub(super) fn inline_note_visible_ranges(
    source: &str,
    source_range: Range<usize>,
) -> Vec<Range<usize>> {
    let Some(text) = source.get(source_range.clone()) else {
        return Vec::new();
    };
    let mut visible = Vec::new();
    let mut cursor = 0usize;
    while cursor < text.len() {
        let remaining = &text[cursor..];
        let next_wiki = remaining.find("[[");
        let next_markdown = remaining.find('[').filter(|relative| {
            let byte = cursor + relative;
            byte == 0 || text.as_bytes().get(byte - 1) != Some(&b'!')
        });
        let next_todo = remaining.find("@todo:");
        let Some(next) = [next_wiki, next_markdown, next_todo]
            .into_iter()
            .flatten()
            .min()
        else {
            visible.push((source_range.start + cursor)..source_range.end);
            break;
        };
        if next > 0 {
            visible.push((source_range.start + cursor)..(source_range.start + cursor + next));
            cursor += next;
            continue;
        }

        if let Some(rest) = text[cursor..].strip_prefix("[[")
            && let Some(end) = rest.find("]]")
        {
            let inner = &rest[..end];
            let (label, offset) = if let Some((_, alias)) = inner.split_once('|') {
                let alias_offset = inner.find('|').unwrap_or(inner.len()) + 1;
                (alias, alias_offset)
            } else {
                (inner, 0)
            };
            let left_trim = label.len() - label.trim_start().len();
            let trimmed_len = label.trim().len();
            if trimmed_len > 0 {
                let start = source_range.start + cursor + 2 + offset + left_trim;
                visible.push(start..start + trimmed_len);
            }
            cursor += 2 + end + 2;
            continue;
        }

        if text[cursor..].starts_with('[')
            && let Some(label_end) = text[cursor + 1..].find("](")
        {
            let label_end = cursor + 1 + label_end;
            let url_start = label_end + 2;
            if let Some(url_end_offset) = text[url_start..].find(')') {
                let url_end = url_start + url_end_offset;
                let url = &text[url_start..url_end];
                if url.starts_with("http://") || url.starts_with("https://") {
                    if label_end > cursor + 1 {
                        visible.push(
                            (source_range.start + cursor + 1)..(source_range.start + label_end),
                        );
                    }
                    cursor = url_end + 1;
                    continue;
                }
            }
        }

        if let Some(rest) = text[cursor..].strip_prefix("@todo:") {
            let id_len = rest
                .bytes()
                .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-')
                .count();
            if id_len > 0 {
                let end = cursor + "@todo:".len() + id_len;
                visible.push((source_range.start + cursor)..(source_range.start + end));
                cursor = end;
                continue;
            }
        }

        let ch_len = text[cursor..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);
        visible.push((source_range.start + cursor)..(source_range.start + cursor + ch_len));
        cursor += ch_len;
    }
    visible
}

fn coalesce_footnote_blocks(
    blocks: &mut Vec<NoteFindPreviewBlock>,
    references: &[(String, Range<usize>)],
    definitions: &[(String, Range<usize>)],
) {
    let mut groups = Vec::<Range<usize>>::new();
    for (label, reference) in references {
        let Some(definition) = definitions
            .iter()
            .find(|(definition_label, _)| definition_label == label)
            .map(|(_, range)| range)
        else {
            continue;
        };
        let reference_index = blocks.iter().position(|block| {
            block.source_range.start <= reference.start && reference.end <= block.source_range.end
        });
        let definition_index = blocks.iter().position(|block| {
            block.source_range.start <= definition.start && definition.end <= block.source_range.end
        });
        if let (Some(reference_index), Some(definition_index)) = (reference_index, definition_index)
        {
            groups.push(
                reference_index.min(definition_index)..reference_index.max(definition_index) + 1,
            );
        }
    }
    groups.sort_by_key(|range| range.start);
    let mut merged_groups = Vec::<Range<usize>>::new();
    for group in groups {
        if let Some(last) = merged_groups.last_mut()
            && group.start <= last.end
        {
            last.end = last.end.max(group.end);
        } else {
            merged_groups.push(group);
        }
    }
    if merged_groups.is_empty() {
        return;
    }

    let old = std::mem::take(blocks);
    let mut cursor = 0;
    let mut next_group = 0;
    while cursor < old.len() {
        let Some(group) = merged_groups.get(next_group) else {
            blocks.extend(old[cursor..].iter().cloned());
            break;
        };
        if cursor < group.start {
            blocks.extend(old[cursor..group.start].iter().cloned());
        }
        let end = group.end.min(old.len());
        let mut visible_ranges = Vec::new();
        for block in &old[group.start..end] {
            visible_ranges.extend(block.visible_ranges.iter().cloned());
        }
        blocks.push(NoteFindPreviewBlock {
            source_range: old[group.start].source_range.start..old[end - 1].source_range.end,
            visible_ranges,
        });
        cursor = end;
        next_group += 1;
    }
}

fn preview_parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_HEADING_ATTRIBUTES
}

static WIKI_TOKEN_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"\[\[[^\]\r\n]+\]\]").expect("wiki link regex is valid")
});
static IMAGE_SOURCE_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").expect("image source regex is valid")
});

fn event_visible_ranges(
    source: &str,
    event: &Event<'_>,
    range: &Range<usize>,
    image_depth: usize,
) -> Vec<Range<usize>> {
    if image_depth > 0 {
        return Vec::new();
    }
    let text = match event {
        Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
            text.as_ref()
        }
        _ => return Vec::new(),
    };
    let Some(raw) = source.get(range.clone()) else {
        return Vec::new();
    };
    if raw == text {
        return vec![range.clone()];
    }
    raw.find(text)
        .map(|start| {
            std::iter::once((range.start + start)..(range.start + start + text.len())).collect()
        })
        .unwrap_or_default()
}

fn subtract_range(ranges: &mut Vec<Range<usize>>, excluded: &Range<usize>) {
    let mut updated = Vec::with_capacity(ranges.len());
    for range in ranges.drain(..) {
        if !ranges_overlap(&range, excluded) {
            updated.push(range);
            continue;
        }
        if range.start < excluded.start {
            updated.push(range.start..excluded.start.min(range.end));
        }
        if excluded.end < range.end {
            updated.push(excluded.end.max(range.start)..range.end);
        }
    }
    *ranges = updated;
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

static TODO_TOKEN_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"@todo:([A-Za-z0-9_-]+)").expect("todo token regex is valid")
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_blocks_keep_lists_tables_code_and_reference_links_whole() {
        let source = "# Start\n\nParagraph [reference][ref].\n\n[ref]: https://example.test\n\n- one\n- two\n\n| A | B |\n| - | - |\n| x | y |\n\n```text\ncode\n```\n";
        let index = NoteFindPreviewIndex::new(source);
        let fragments: Vec<_> = index
            .blocks
            .iter()
            .map(|block| index.rendered_block(source, block))
            .collect();
        assert!(fragments.iter().any(|block| block.contains("- one\n- two")));
        assert!(fragments.iter().any(|block| block.contains("| x | y |")));
        assert!(
            fragments
                .iter()
                .any(|block| block.contains("```text\ncode\n```"))
        );
        assert!(
            fragments
                .iter()
                .any(|block| block.contains("[ref]: https://example.test"))
        );
        assert_eq!(index.blocks.len(), 5);
    }

    #[test]
    fn source_visibility_matches_commonmark_and_custom_task_renderers() {
        let source = "**visible** [label](https://hidden.test) ![alt](image.png) ![](fallback.png|240) @todo:resolved-id @todo:fallback-id [[hidden-target|shown-alias]]\n\n[ref]: https://definition.test";
        let index = NoteFindPreviewIndex::new(source);
        let mut labels = std::collections::HashMap::new();
        labels.insert("resolved-id".to_owned(), "Readable title".to_owned());
        let commonmark_visible = |query: &str, images_as_links: bool| {
            let start = source.find(query).unwrap();
            let range = start..start + query.len();
            index.match_is_visible_in(source, &range, "current", &labels, images_as_links)
        };

        assert!(commonmark_visible("visible", false));
        assert!(commonmark_visible("label", false));
        assert!(!commonmark_visible("https://hidden.test", false));
        assert!(!commonmark_visible("alt", false));
        assert!(commonmark_visible("alt", true));
        assert!(commonmark_visible("fallback.png", true));
        assert!(
            !commonmark_visible("resolved-id", false),
            "a resolved todo id must be source-only; blocks={:?}, visible={:?}",
            index.blocks,
            index
                .blocks
                .iter()
                .flat_map(|block| {
                    index.visible_ranges_for_block(source, block, "current", &labels, false)
                })
                .collect::<Vec<_>>(),
        );
        assert!(commonmark_visible("fallback-id", false));
        assert!(commonmark_visible("hidden-target", false));
        assert!(commonmark_visible("shown-alias", false));
        assert!(!commonmark_visible("[[", false));
        assert!(!commonmark_visible("https://definition.test", false));
        assert!(!commonmark_visible("**", false));

        let task_source = "task ![alt](image.png) @todo:resolved-id [[hidden-target|shown-alias]] [web](https://hidden.test)";
        let task_visible = inline_note_visible_ranges(task_source, 0..task_source.len());
        let task_has = |query: &str| {
            let start = task_source.find(query).unwrap();
            task_visible
                .iter()
                .any(|range| contains(range, &(start..start + query.len())))
        };
        assert!(task_has("@todo:resolved-id"));
        assert!(!task_has("hidden-target"));
        assert!(task_has("shown-alias"));
        assert!(!task_has("https://hidden.test"));
        // The custom task-row renderer treats image markup as ordinary label
        // text, unlike the normal image widget path.
        assert!(task_has("image.png"));
    }

    #[test]
    fn footnote_documents_keep_semantic_body_blocks() {
        let source = "A note[^one].\n\n[^one]: Footnote body.";
        let index = NoteFindPreviewIndex::new(source);
        assert!(index.has_footnotes());
        assert_eq!(index.blocks.len(), 1);
        assert_eq!(index.blocks[0].source_range, 0..source.len());
    }

    #[test]
    fn wiki_link_brackets_are_source_syntax_but_label_text_is_visible() {
        let source = "[[target|alias]]";
        let index = NoteFindPreviewIndex::new(source);
        let visible = |query: &str| {
            let start = source.find(query).unwrap();
            let range = start..start + query.len();
            index.match_is_visible_in(
                source,
                &range,
                "current",
                &std::collections::HashMap::new(),
                false,
            )
        };
        assert!(visible("target"));
        assert!(visible("alias"));
        assert!(!visible("[["));
        assert!(!visible("]]"));
    }

    fn contains(outer: &Range<usize>, inner: &Range<usize>) -> bool {
        outer.start <= inner.start && inner.end <= outer.end
    }
}
