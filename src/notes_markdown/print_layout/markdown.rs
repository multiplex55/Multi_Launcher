//! Source-bound, bounded Markdown structure used by print layout.
//!
//! Nodes form an arena: children are indices, so traversal and destruction do
//! not recurse through attacker-controlled Markdown nesting.
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

use super::MAX_TEXT_BYTES;

const MAX_EVENTS: usize = 100_000;
const MAX_DEPTH: usize = 128;
const MAX_OWNED_BYTES: usize = 8 * MAX_TEXT_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ParseError {
    SourceTooLarge,
    EventLimit,
    DepthLimit,
    AllocationLimit,
    Cancelled,
    InvalidStructure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ColumnAlignment {
    Default,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CalloutHeader {
    pub kind: String,
    pub title: String,
    pub source: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NodeKind {
    Paragraph,
    Heading {
        level: u8,
        anchor: Option<String>,
    },
    Quote {
        callout: Option<CalloutHeader>,
    },
    List {
        first: Option<u64>,
    },
    Item,
    Table {
        alignments: Vec<ColumnAlignment>,
    },
    TableHead,
    TableRow,
    TableCell,
    CodeBlock {
        language: Option<String>,
    },
    FootnoteDefinition(String),
    Emphasis,
    Strong,
    Strike,
    Link {
        destination: String,
        title: String,
    },
    Image {
        destination: String,
        title: String,
    },
    Text(String),
    Code(String),
    SoftBreak,
    HardBreak,
    Task(bool),
    Rule,
    FootnoteReference(String),
    PageBreak,
    /// Raw HTML is preserved as visible source and must not be trusted as markup.
    Unsupported(String),
    // Internal sentinel removed from the reachable tree at close.
    HtmlBlock,
    Ignored,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Node {
    pub kind: NodeKind,
    pub source: Range<usize>,
    pub children: Vec<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Warning {
    pub source: Range<usize>,
    pub message: &'static str,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ParsedMarkdown {
    pub nodes: Vec<Node>,
    pub roots: Vec<usize>,
    pub warnings: Vec<Warning>,
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
}
fn comment(raw: &str) -> bool {
    let raw = raw.trim();
    raw.strip_prefix("<!--")
        .and_then(|body| body.strip_suffix("-->"))
        .is_some_and(|body| !body.contains("-->"))
}
fn pagebreak(source: &str, range: &Range<usize>) -> bool {
    let raw = &source[range.clone()];
    (range.start == 0 || source.as_bytes()[range.start - 1] == b'\n')
        && raw.trim_end_matches(['\r', '\n']) == "<!-- pagebreak -->"
        && (range.end == source.len()
            || source.as_bytes().get(range.end.wrapping_sub(1)) == Some(&b'\n'))
}
fn quote_callout(source: &str, range: &Range<usize>) -> Option<CalloutHeader> {
    let first = source[range.clone()].split_inclusive('\n').next()?;
    let (kind, title) =
        super::super::callouts::callout_header(first.trim_end_matches(['\r', '\n']))?;
    Some(CalloutHeader {
        kind,
        title,
        source: range.start..range.start + first.len(),
    })
}
fn align(value: Alignment) -> ColumnAlignment {
    match value {
        Alignment::None => ColumnAlignment::Default,
        Alignment::Left => ColumnAlignment::Left,
        Alignment::Center => ColumnAlignment::Center,
        Alignment::Right => ColumnAlignment::Right,
    }
}

/// Parse the original source without altering it. `cancelled` is checked before
/// and throughout the bounded event walk and before/after heading resolution.
pub(super) fn parse(
    source: &str,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ParsedMarkdown, ParseError> {
    if cancelled() {
        return Err(ParseError::Cancelled);
    }
    if source.len() > MAX_TEXT_BYTES {
        return Err(ParseError::SourceTooLarge);
    }
    let start = super::super::headings::initial_metadata_end(source);
    let mut parsed = ParsedMarkdown {
        nodes: Vec::new(),
        roots: Vec::new(),
        warnings: Vec::new(),
    };
    let mut stack: Vec<(usize, TagEnd)> = Vec::new();
    let mut owned = 0usize;
    let mut top_headings = Vec::new();
    for (index, (event, relative)) in Parser::new_ext(&source[start..], options())
        .into_offset_iter()
        .enumerate()
    {
        if cancelled() {
            return Err(ParseError::Cancelled);
        }
        if index >= MAX_EVENTS {
            return Err(ParseError::EventLimit);
        }
        let range = start + relative.start..start + relative.end;
        if range.start > range.end
            || range.end > source.len()
            || !source.is_char_boundary(range.start)
            || !source.is_char_boundary(range.end)
        {
            return Err(ParseError::InvalidStructure);
        }
        match event {
            Event::Start(tag) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(ParseError::DepthLimit);
                }
                let end = tag.to_end();
                let kind = match tag {
                    Tag::Paragraph => NodeKind::Paragraph,
                    Tag::Heading { level, .. } => NodeKind::Heading {
                        level: level as u8,
                        anchor: None,
                    },
                    Tag::BlockQuote(_) => NodeKind::Quote {
                        callout: quote_callout(source, &range),
                    },
                    Tag::List(first) => NodeKind::List { first },
                    Tag::Item => NodeKind::Item,
                    Tag::Table(a) => NodeKind::Table {
                        alignments: a.into_iter().map(align).collect(),
                    },
                    Tag::TableHead => NodeKind::TableHead,
                    Tag::TableRow => NodeKind::TableRow,
                    Tag::TableCell => NodeKind::TableCell,
                    Tag::CodeBlock(kind) => NodeKind::CodeBlock {
                        language: match kind {
                            CodeBlockKind::Indented => None,
                            CodeBlockKind::Fenced(info) => Some(info.to_string()),
                        },
                    },
                    Tag::HtmlBlock => NodeKind::HtmlBlock,
                    Tag::FootnoteDefinition(label) => {
                        NodeKind::FootnoteDefinition(label.to_string())
                    }
                    Tag::Emphasis => NodeKind::Emphasis,
                    Tag::Strong => NodeKind::Strong,
                    Tag::Strikethrough => NodeKind::Strike,
                    Tag::Link {
                        dest_url, title, ..
                    } => NodeKind::Link {
                        destination: dest_url.to_string(),
                        title: title.to_string(),
                    },
                    Tag::Image {
                        dest_url, title, ..
                    } => NodeKind::Image {
                        destination: dest_url.to_string(),
                        title: title.to_string(),
                    },
                    // Disabled extensions should not occur; preserve their source if they do.
                    _ => {
                        parsed.warnings.push(Warning {
                            source: range.clone(),
                            message: "Unsupported Markdown construct",
                        });
                        NodeKind::Unsupported(source[range.clone()].to_string())
                    }
                };
                let node = parsed.nodes.len();
                if stack.is_empty() && matches!(kind, NodeKind::Heading { .. }) {
                    top_headings.push(node);
                }
                append(&mut parsed, &stack, kind, range.clone(), &mut owned)?;
                stack.push((node, end));
            }
            Event::End(end) => {
                let (node, expected) = stack.pop().ok_or(ParseError::InvalidStructure)?;
                if expected != end {
                    return Err(ParseError::InvalidStructure);
                }
                if matches!(parsed.nodes[node].kind, NodeKind::HtmlBlock) {
                    let raw_range = parsed.nodes[node].source.clone();
                    let raw = &source[raw_range.clone()];
                    parsed.nodes[node].children.clear();
                    parsed.nodes[node].kind = if stack.is_empty() && pagebreak(source, &raw_range) {
                        NodeKind::PageBreak
                    } else if comment(raw) {
                        NodeKind::Ignored
                    } else {
                        owned = owned
                            .checked_add(raw.len())
                            .ok_or(ParseError::AllocationLimit)?;
                        if owned > MAX_OWNED_BYTES {
                            return Err(ParseError::AllocationLimit);
                        }
                        parsed.warnings.push(Warning {
                            source: raw_range,
                            message: "Raw HTML is printed as source",
                        });
                        NodeKind::Unsupported(raw.to_string())
                    };
                }
            }
            // HTML blocks are classified as a whole at their end.
            Event::Html(_)
                if stack
                    .last()
                    .is_some_and(|(i, _)| matches!(parsed.nodes[*i].kind, NodeKind::HtmlBlock)) => {
            }
            Event::Html(raw) | Event::InlineHtml(raw) => {
                if !comment(&raw) {
                    parsed.warnings.push(Warning {
                        source: range.clone(),
                        message: "Raw HTML is printed as source",
                    });
                    append(
                        &mut parsed,
                        &stack,
                        NodeKind::Unsupported(raw.to_string()),
                        range,
                        &mut owned,
                    )?;
                }
            }
            Event::Text(text) => append(
                &mut parsed,
                &stack,
                NodeKind::Text(text.to_string()),
                range,
                &mut owned,
            )?,
            Event::Code(text) => append(
                &mut parsed,
                &stack,
                NodeKind::Code(text.to_string()),
                range,
                &mut owned,
            )?,
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                parsed.warnings.push(Warning {
                    source: range.clone(),
                    message: "Unsupported math is printed as source",
                });
                append(
                    &mut parsed,
                    &stack,
                    NodeKind::Unsupported(text.to_string()),
                    range,
                    &mut owned,
                )?;
            }
            Event::FootnoteReference(label) => append(
                &mut parsed,
                &stack,
                NodeKind::FootnoteReference(label.to_string()),
                range,
                &mut owned,
            )?,
            Event::SoftBreak => {
                append(&mut parsed, &stack, NodeKind::SoftBreak, range, &mut owned)?
            }
            Event::HardBreak => {
                append(&mut parsed, &stack, NodeKind::HardBreak, range, &mut owned)?
            }
            Event::Rule => append(&mut parsed, &stack, NodeKind::Rule, range, &mut owned)?,
            Event::TaskListMarker(checked) => append(
                &mut parsed,
                &stack,
                NodeKind::Task(checked),
                range,
                &mut owned,
            )?,
        }
    }
    if !stack.is_empty() {
        return Err(ParseError::InvalidStructure);
    }
    if cancelled() {
        return Err(ParseError::Cancelled);
    }
    let headings = if top_headings.is_empty() {
        Vec::new()
    } else {
        super::super::headings::structural_headings(source)
    };
    if cancelled() {
        return Err(ParseError::Cancelled);
    }
    for node in top_headings {
        let position = parsed.nodes[node].source.start;
        let index = headings.partition_point(|heading| heading.byte_range.end <= position);
        if let Some(heading) = headings
            .get(index)
            .filter(|heading| heading.byte_range.contains(&position))
            && let NodeKind::Heading { anchor, .. } = &mut parsed.nodes[node].kind
        {
            if !heading.normalized_anchor.is_empty() {
                owned = owned
                    .checked_add(heading.normalized_anchor.len())
                    .ok_or(ParseError::AllocationLimit)?;
                if owned > MAX_OWNED_BYTES {
                    return Err(ParseError::AllocationLimit);
                }
                *anchor = Some(heading.normalized_anchor.clone());
            }
        }
    }
    Ok(parsed)
}

fn append(
    parsed: &mut ParsedMarkdown,
    stack: &[(usize, TagEnd)],
    kind: NodeKind,
    source: Range<usize>,
    owned: &mut usize,
) -> Result<(), ParseError> {
    let bytes = match &kind {
        NodeKind::Text(s)
        | NodeKind::Code(s)
        | NodeKind::Unsupported(s)
        | NodeKind::FootnoteReference(s)
        | NodeKind::FootnoteDefinition(s) => s.len(),
        NodeKind::Link { destination, title } | NodeKind::Image { destination, title } => {
            destination.len().saturating_add(title.len())
        }
        NodeKind::CodeBlock { language: Some(s) } => s.len(),
        NodeKind::Quote {
            callout: Some(header),
        } => header.kind.len().saturating_add(header.title.len()),
        NodeKind::Table { alignments } => alignments
            .len()
            .saturating_mul(std::mem::size_of::<ColumnAlignment>()),
        _ => 0,
    };
    *owned = owned
        .checked_add(bytes)
        .ok_or(ParseError::AllocationLimit)?;
    if *owned > MAX_OWNED_BYTES {
        return Err(ParseError::AllocationLimit);
    }
    let index = parsed.nodes.len();
    parsed.nodes.push(Node {
        kind,
        source,
        children: Vec::new(),
    });
    if let Some((parent, _)) = stack.last() {
        parsed.nodes[*parent].children.push(index);
    } else {
        parsed.roots.push(index);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn all(source: &str) -> ParsedMarkdown {
        parse(source, || false).unwrap()
    }
    fn kinds(parsed: &ParsedMarkdown) -> impl Iterator<Item = &NodeKind> {
        parsed.nodes.iter().map(|node| &node.kind)
    }

    #[test]
    fn print_markdown_keeps_rich_blocks_and_inline_meaning() {
        let source = "# Same\n\n# Same\n\nParagraph *em* **strong** ~~strike~~ `code` [link](https://example.test) ![alt](image.png)  \nnext\n\n3. [x] done\n   - child\n\n> [!NOTE] Title\n> content\n\n| A | B |\n| :-- | --: |\n| x | y |\n\n---\n\n```mermaid\ngraph TD\n```\n";
        let parsed = all(source);
        let anchors: Vec<_> = kinds(&parsed)
            .filter_map(|kind| match kind {
                NodeKind::Heading { anchor, .. } => anchor.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(anchors, vec!["same", "same-1"]);
        assert!(kinds(&parsed).any(|k| matches!(k, NodeKind::Strong)));
        assert!(kinds(&parsed).any(|k| matches!(k, NodeKind::Strike)));
        assert!(kinds(&parsed).any(|k| matches!(k, NodeKind::HardBreak)));
        assert!(kinds(&parsed).any(|k| matches!(k, NodeKind::Link { destination, .. } if destination == "https://example.test")));
        assert!(kinds(&parsed).any(
            |k| matches!(k, NodeKind::Image { destination, .. } if destination == "image.png")
        ));
        assert!(kinds(&parsed).any(|k| matches!(k, NodeKind::List { first: Some(3) })));
        assert!(kinds(&parsed).any(|k| matches!(k, NodeKind::Task(true))));
        assert!(kinds(&parsed).any(|k| matches!(k, NodeKind::Quote { callout: Some(header) } if header.kind == "note" && header.title == "Title")));
        assert!(kinds(&parsed).any(|k| matches!(k, NodeKind::Table { alignments } if alignments == &[ColumnAlignment::Left, ColumnAlignment::Right])));
        assert!(kinds(&parsed).any(|k| matches!(k, NodeKind::CodeBlock { language: Some(language) } if language == "mermaid")));
        assert!(
            kinds(&parsed).any(|k| matches!(k, NodeKind::Text(text) if text.contains("graph TD")))
        );
    }

    #[test]
    fn print_markdown_authenticates_breaks_and_excludes_metadata() {
        let source = "---\r\ntitle: note\r\n<!-- pagebreak -->\r\n---\r\n\r\n<!-- pagebreak -->\r\n\r\n > <!-- pagebreak -->\r\n\r\ntext <!-- pagebreak -->\r\n\r\n<!-- pagebreak --> trailing\r\n\r\n    <!-- pagebreak -->\r\n\r\n```\r\n<!-- pagebreak -->\r\n```\r\n\r\n\\<!-- pagebreak -->\r\n\r\n<div>visible</div>\r\n";
        let parsed = all(source);
        assert_eq!(
            kinds(&parsed)
                .filter(|k| matches!(k, NodeKind::PageBreak))
                .count(),
            1
        );
        let node = parsed
            .nodes
            .iter()
            .find(|node| matches!(node.kind, NodeKind::PageBreak))
            .unwrap();
        assert_eq!(&source[node.source.clone()], "<!-- pagebreak -->\r\n");
        assert!(
            kinds(&parsed)
                .any(|k| matches!(k, NodeKind::Unsupported(raw) if raw.contains("<div>")))
        );
        assert!(!parsed.warnings.is_empty());
        let larger =
            all("<div>\n<!-- pagebreak -->\n</div>\n\n<!-- outer\n<!-- pagebreak -->\n-->\n");
        assert!(!kinds(&larger).any(|k| matches!(k, NodeKind::PageBreak)));
        let eof = all("before\n\n<!-- pagebreak -->");
        assert_eq!(
            kinds(&eof)
                .filter(|k| matches!(k, NodeKind::PageBreak))
                .count(),
            1
        );
        for raw in ["<!-->", "<!--->", "text <!--> tail", "text <!---> tail"] {
            let _ = all(raw);
        }
        let empty = all("#\n");
        assert!(kinds(&empty).any(|k| matches!(k, NodeKind::Heading { anchor: None, .. })));
    }

    #[test]
    fn print_markdown_limits_and_cancellation_are_explicit() {
        assert_eq!(
            parse(&"x".repeat(MAX_TEXT_BYTES + 1), || false),
            Err(ParseError::SourceTooLarge)
        );
        assert_eq!(parse("anything", || true), Err(ParseError::Cancelled));
        let mut calls = 0;
        assert_eq!(
            parse("one *two* three", || {
                calls += 1;
                calls > 3
            }),
            Err(ParseError::Cancelled)
        );
        let deeply_nested = format!("{}x\n", "> ".repeat(MAX_DEPTH + 2));
        assert_eq!(parse(&deeply_nested, || false), Err(ParseError::DepthLimit));
        let events = "*x* ".repeat(MAX_EVENTS / 2);
        assert_eq!(parse(&events, || false), Err(ParseError::EventLimit));
    }
}
