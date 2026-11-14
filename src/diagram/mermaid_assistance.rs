//! Bounded, parser-aware Mermaid completions for the supported flowchart subset.

use std::collections::HashSet;
use std::ops::Range;

use super::mermaid::{
    MAX_MERMAID_SOURCE_BYTES, SourcePieceKind, assistance_edge_target_span, assistance_symbols,
    parse_header, preflight_source, scan_source,
};
use super::model::{MAX_DIAGRAM_GROUPS, MAX_DIAGRAM_NODES};

const MAX_COMPLETIONS: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MermaidAssistanceContext {
    Header,
    Direction,
    Statement,
    EdgeTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MermaidAssistanceSuggestion {
    pub label: String,
    pub insertion: String,
    pub replacement: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MermaidAssistance {
    pub context: MermaidAssistanceContext,
    pub suggestions: Vec<MermaidAssistanceSuggestion>,
}

/// Find a small set of explicit completions at a byte cursor in the current
/// source draft. This only scans the current line and, for endpoint IDs, the
/// bounded grammar-recognized identifiers already present in the draft; it
/// does not build a graph or rewrite the document.
pub fn mermaid_assistance_at(source: &str, cursor_byte: usize) -> Option<MermaidAssistance> {
    if source.len() > MAX_MERMAID_SOURCE_BYTES
        || preflight_source(source).is_err()
        || cursor_byte > source.len()
        || !source.is_char_boundary(cursor_byte)
    {
        return None;
    }

    let line_start = source[..cursor_byte]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let line_end = source[cursor_byte..]
        .find('\n')
        .map_or(source.len(), |offset| cursor_byte + offset);
    let raw_line = &source[line_start..line_end];
    let content_end = raw_line.strip_suffix('\r').map_or(raw_line.len(), str::len);
    let physical_line = &raw_line[..content_end];
    let physical_cursor = cursor_byte.saturating_sub(line_start).min(content_end);

    if physical_line.trim_start().starts_with("%%")
        || cursor_is_inside_literal(physical_line, physical_cursor)
    {
        return None;
    }

    let pieces = scan_source(physical_line);
    let current = pieces
        .iter()
        .find(|piece| piece.range.start <= physical_cursor && physical_cursor <= piece.range.end);
    let range = current.map_or(physical_cursor..physical_cursor, |piece| {
        piece.range.clone()
    });
    let line_start = line_start + range.start;
    let line = &physical_line[range.clone()];
    let cursor = physical_cursor - range.start;

    let Some((header_start, header_end)) = first_nonempty_line(source) else {
        return header_assistance(line_start, line, cursor)
            .and_then(|assistance| bounded_assistance(source, assistance));
    };
    if line_start == header_start && line_start + line.len() == header_end {
        return header_assistance(line_start, line, cursor)
            .and_then(|assistance| bounded_assistance(source, assistance));
    }
    if line_start < header_end {
        return None;
    }
    if parse_header(&source[header_start..header_end], header_start, 1).is_err() {
        return None;
    }

    if let Some((start, end)) = assistance_edge_target_span(line, cursor) {
        let range = start..end;
        let fragment = &line[range.clone()];
        let (identifiers, _, _) = assistance_symbols(source);
        let suggestions = identifiers
            .into_iter()
            .filter(|id| id.starts_with(fragment) && id != fragment)
            .take(MAX_COMPLETIONS)
            .map(|id| {
                suggestion(
                    id.clone(),
                    id,
                    line_start + range.start..line_start + range.end,
                )
            })
            .collect::<Vec<_>>();
        if !suggestions.is_empty() {
            return bounded_assistance(
                source,
                MermaidAssistance {
                    context: MermaidAssistanceContext::EdgeTarget,
                    suggestions,
                },
            );
        }
    }

    if line[..cursor].trim().is_empty() && line[cursor..].trim().is_empty() {
        let replacement = line_start + cursor..line_start + cursor;
        let (ids, group_ids, edge_count) = assistance_symbols(source);
        let node_id = next_node_id(&ids);
        let edge_from = ids.first().cloned().unwrap_or_else(|| node_id.clone());
        let edge_to = if ids.len() > 1 {
            ids[1].clone()
        } else {
            next_node_id_with_seed(&ids, &edge_from)
        };
        let group_id = next_group_id(&group_ids);
        let newline = source_newline(source);
        let mut suggestions = Vec::new();
        if ids.len() < MAX_DIAGRAM_NODES {
            suggestions.push(suggestion(
                format!("Declare {node_id}"),
                format!("{node_id}[\"New node\"]"),
                replacement.clone(),
            ));
        }
        if edge_count < super::model::MAX_DIAGRAM_EDGES && ids.len() >= 2 {
            suggestions.push(suggestion(
                "Connect nodes".into(),
                format!("{edge_from} --> {edge_to}"),
                replacement.clone(),
            ));
        }
        if !has_open_subgraph(&source[..line_start]) && group_ids.len() < MAX_DIAGRAM_GROUPS {
            suggestions.push(suggestion(
                "Subgraph".into(),
                format!("subgraph {group_id}[\"Group\"]{newline}end"),
                replacement.clone(),
            ));
        } else if has_open_subgraph(&source[..line_start])
            && !has_closing_end(&source[line_start + line.len()..])
        {
            suggestions.push(suggestion("End subgraph".into(), "end".into(), replacement));
        }
        if suggestions.is_empty() {
            return None;
        }
        return bounded_assistance(
            source,
            MermaidAssistance {
                context: MermaidAssistanceContext::Statement,
                suggestions,
            },
        );
    }

    None
}

/// Apply one explicit suggestion while preserving every byte outside its
/// bounded replacement span. The returned cursor is a UTF-8 byte offset.
pub fn apply_mermaid_assistance(
    source: &str,
    completion: &MermaidAssistanceSuggestion,
) -> Option<(String, usize)> {
    let range = completion.replacement.clone();
    if range.start > range.end
        || range.end > source.len()
        || !source.is_char_boundary(range.start)
        || !source.is_char_boundary(range.end)
        || source
            .len()
            .saturating_sub(range.end - range.start)
            .saturating_add(completion.insertion.len())
            > MAX_MERMAID_SOURCE_BYTES
    {
        return None;
    }
    let mut updated = String::with_capacity(
        source
            .len()
            .saturating_sub(range.end - range.start)
            .saturating_add(completion.insertion.len()),
    );
    updated.push_str(&source[..range.start]);
    updated.push_str(&completion.insertion);
    updated.push_str(&source[range.end..]);
    if preflight_source(&updated).is_err() {
        return None;
    }
    let cursor = range.start + completion.insertion.len();
    Some((updated, cursor))
}

fn bounded_assistance(
    source: &str,
    mut assistance: MermaidAssistance,
) -> Option<MermaidAssistance> {
    assistance
        .suggestions
        .retain(|suggestion| apply_mermaid_assistance(source, suggestion).is_some());
    (!assistance.suggestions.is_empty()).then_some(assistance)
}

fn header_assistance(line_start: usize, line: &str, cursor: usize) -> Option<MermaidAssistance> {
    let leading = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Some(MermaidAssistance {
            context: MermaidAssistanceContext::Header,
            suggestions: ["TD", "TB", "LR", "RL", "BT"]
                .into_iter()
                .map(|direction| {
                    suggestion(
                        format!("Flowchart {direction}"),
                        format!("flowchart {direction}"),
                        line_start + cursor..line_start + cursor,
                    )
                })
                .collect(),
        });
    }

    let token_start = leading;
    if cursor < token_start {
        return None;
    }
    let token_end = line[token_start..]
        .find(char::is_whitespace)
        .map_or(line.len(), |offset| token_start + offset);
    let first_token = &line[token_start..token_end];
    if first_token != "flowchart"
        && first_token != "graph"
        && ("flowchart".starts_with(first_token) || "graph".starts_with(first_token))
        && cursor >= token_start
        && cursor <= token_end
        && !first_token.is_empty()
    {
        let range = line_start + token_start..line_start + token_end;
        return Some(MermaidAssistance {
            context: MermaidAssistanceContext::Header,
            suggestions: ["TD", "TB", "LR", "RL", "BT"]
                .into_iter()
                .map(|direction| {
                    suggestion(
                        format!("flowchart {direction}"),
                        format!("flowchart {direction}"),
                        range.clone(),
                    )
                })
                .collect(),
        });
    }

    if first_token != "flowchart" && first_token != "graph" {
        return None;
    }
    let direction_start = token_end;
    if direction_start == line.len() {
        if cursor != direction_start {
            return None;
        }
        let replacement = line_start + token_start..line_start + token_end;
        return Some(MermaidAssistance {
            context: MermaidAssistanceContext::Direction,
            suggestions: ["TD", "TB", "LR", "RL", "BT"]
                .into_iter()
                .map(|direction| {
                    suggestion(
                        direction.into(),
                        format!("{first_token} {direction}"),
                        replacement.clone(),
                    )
                })
                .collect(),
        });
    }
    if cursor < direction_start {
        return None;
    }
    let whitespace_end = line[direction_start..]
        .find(|character: char| !character.is_whitespace())
        .map_or(line.len(), |offset| direction_start + offset);
    if whitespace_end == direction_start {
        return None;
    }
    let direction_end = line[whitespace_end..]
        .find(char::is_whitespace)
        .map_or(line.len(), |offset| whitespace_end + offset);
    if cursor < whitespace_end || cursor > direction_end {
        return None;
    }
    let direction_fragment = &line[whitespace_end..direction_end];
    let direction_prefix = &line[whitespace_end..cursor.min(direction_end)];
    if direction_fragment.is_empty() {
        return Some(MermaidAssistance {
            context: MermaidAssistanceContext::Direction,
            suggestions: ["TD", "TB", "LR", "RL", "BT"]
                .into_iter()
                .map(|direction| {
                    suggestion(
                        direction.into(),
                        direction.into(),
                        line_start + whitespace_end..line_start + whitespace_end,
                    )
                })
                .collect(),
        });
    }
    let complete_direction = matches!(direction_fragment, "TD" | "TB" | "LR" | "RL" | "BT");
    let matching_directions = ["TD", "TB", "LR", "RL", "BT"]
        .into_iter()
        .filter(|direction| {
            if complete_direction {
                *direction != direction_fragment
            } else {
                direction.starts_with(direction_prefix)
            }
        });
    let suggestions = matching_directions
        .map(|direction| {
            suggestion(
                direction.into(),
                direction.into(),
                line_start + whitespace_end..line_start + direction_end,
            )
        })
        .collect::<Vec<_>>();
    if !suggestions.is_empty() {
        return Some(MermaidAssistance {
            context: MermaidAssistanceContext::Direction,
            suggestions,
        });
    }
    None
}

fn suggestion(
    label: String,
    insertion: String,
    replacement: Range<usize>,
) -> MermaidAssistanceSuggestion {
    MermaidAssistanceSuggestion {
        label,
        insertion,
        replacement,
    }
}

fn first_nonempty_line(source: &str) -> Option<(usize, usize)> {
    scan_source(source)
        .into_iter()
        .find(|piece| {
            piece.kind == SourcePieceKind::Statement
                && !source[piece.range.clone()].trim().is_empty()
        })
        .map(|piece| (piece.range.start, piece.range.end))
}

fn cursor_is_inside_literal(line: &str, cursor: usize) -> bool {
    let mut quoted = false;
    let mut edge_label = false;
    for character in line[..cursor].chars() {
        match character {
            '"' if !edge_label => quoted = !quoted,
            '|' if !quoted => edge_label = !edge_label,
            _ => {}
        }
    }
    quoted || edge_label
}

fn next_node_id(ids: &[String]) -> String {
    next_node_id_with_seed(ids, "Node")
}

fn next_node_id_with_seed(ids: &[String], seed: &str) -> String {
    let used = ids.iter().map(String::as_str).collect::<HashSet<_>>();
    if !used.contains(seed) {
        return seed.to_owned();
    }
    (2usize..)
        .map(|index| format!("{seed}{index}"))
        .find(|candidate| !used.contains(candidate.as_str()))
        .unwrap_or_else(|| "Node".into())
}

fn next_group_id(ids: &[String]) -> String {
    next_node_id_with_seed(ids, "Group")
}

fn has_open_subgraph(source: &str) -> bool {
    let mut open = false;
    for piece in scan_source(source) {
        if piece.kind != SourcePieceKind::Statement {
            continue;
        }
        let line = source[piece.range].trim();
        if line == "end" {
            open = false;
        } else if line == "subgraph"
            || line.starts_with("subgraph ")
            || line.starts_with("subgraph\t")
        {
            open = true;
        }
    }
    open
}

fn has_closing_end(source: &str) -> bool {
    scan_source(source).into_iter().any(|piece| {
        piece.kind == SourcePieceKind::Statement && source[piece.range].trim() == "end"
    })
}

fn source_newline(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::{MermaidAssistanceContext, apply_mermaid_assistance, mermaid_assistance_at};
    use crate::diagram::parse_mermaid;

    #[test]
    fn mermaid_assistance_recognizes_supported_headers_and_directions() {
        let header = mermaid_assistance_at("flow", 4).unwrap();
        assert_eq!(header.context, MermaidAssistanceContext::Header);
        assert_eq!(header.suggestions[0].insertion, "flowchart TD");

        let direction = mermaid_assistance_at("flowchart ", 10).unwrap();
        assert_eq!(direction.context, MermaidAssistanceContext::Direction);
        assert_eq!(
            direction
                .suggestions
                .iter()
                .map(|suggestion| suggestion.insertion.as_str())
                .collect::<Vec<_>>(),
            ["TD", "TB", "LR", "RL", "BT"]
        );

        let complete = "flowchart TD";
        let direction = mermaid_assistance_at(complete, complete.len()).unwrap();
        assert_eq!(direction.context, MermaidAssistanceContext::Direction);
        assert_eq!(
            direction
                .suggestions
                .iter()
                .map(|suggestion| suggestion.insertion.as_str())
                .collect::<Vec<_>>(),
            ["TB", "LR", "RL", "BT"]
        );
        let (replaced, _) = apply_mermaid_assistance(complete, &direction.suggestions[0]).unwrap();
        assert_eq!(replaced, "flowchart TB");

        assert!(mermaid_assistance_at("  flowchart TD", 0).is_none());
        let missing_direction = mermaid_assistance_at("flowchart", "flowchart".len()).unwrap();
        assert!(missing_direction.suggestions.iter().any(|suggestion| {
            let (updated, _) = apply_mermaid_assistance("flowchart", suggestion).unwrap();
            updated == "flowchart TD"
        }));
        assert!(
            mermaid_assistance_at("flowchart ", "flowchart ".len())
                .unwrap()
                .suggestions
                .iter()
                .any(|suggestion| suggestion.insertion == "TB")
        );
    }

    #[test]
    fn mermaid_assistance_offers_supported_snippets_and_current_ids() {
        let blank = "flowchart TD\nA[\"Alpha\"]\n";
        let statement = mermaid_assistance_at(blank, blank.len()).unwrap();
        assert_eq!(statement.context, MermaidAssistanceContext::Statement);
        let node = statement
            .suggestions
            .iter()
            .find(|suggestion| suggestion.label.starts_with("Declare"))
            .unwrap();
        let (with_node, _) = apply_mermaid_assistance(blank, node).unwrap();
        assert!(parse_mermaid(&with_node).is_ok());

        let incomplete_edge = "flowchart TD\nA[\"Alpha\"]\nB\nA --> ";
        let edge = mermaid_assistance_at(incomplete_edge, incomplete_edge.len()).unwrap();
        assert_eq!(edge.context, MermaidAssistanceContext::EdgeTarget);
        assert_eq!(
            edge.suggestions
                .iter()
                .map(|suggestion| suggestion.insertion.as_str())
                .collect::<Vec<_>>(),
            ["A", "B"]
        );
        let (completed_edge, _) =
            apply_mermaid_assistance(incomplete_edge, &edge.suggestions[1]).unwrap();
        assert!(parse_mermaid(&completed_edge).is_ok());

        let compact_arrow = "flowchart TD\nA-->B\nC[\"Gamma\"]\nA-->";
        let edge = mermaid_assistance_at(compact_arrow, compact_arrow.len()).unwrap();
        assert_eq!(edge.context, MermaidAssistanceContext::EdgeTarget);
        assert!(
            edge.suggestions
                .iter()
                .any(|suggestion| suggestion.insertion == "B")
        );
        assert!(
            !edge
                .suggestions
                .iter()
                .any(|suggestion| suggestion.insertion == "A--")
        );

        let prefix = "flowchart TD\nBeta[\"Beta\"]\nGamma\nStart --> Be";
        let edge = mermaid_assistance_at(prefix, prefix.len()).unwrap();
        assert_eq!(
            edge.suggestions
                .iter()
                .map(|suggestion| suggestion.insertion.as_str())
                .collect::<Vec<_>>(),
            ["Beta"]
        );

        let labeled = "flowchart TD\nBeta[\"Beta\"]\nStart -->|arrow --> in label| Be";
        let edge = mermaid_assistance_at(labeled, labeled.len()).unwrap();
        assert_eq!(edge.context, MermaidAssistanceContext::EdgeTarget);
        assert_eq!(edge.suggestions[0].insertion, "Beta");

        for completed_or_invalid in [
            "flowchart TD\nA --> B[\"Label\"]",
            "flowchart TD\nA --> B --> C",
            "flowchart TD\nclassDef alpha fill:#fff\nStart --> al",
        ] {
            assert!(
                mermaid_assistance_at(completed_or_invalid, completed_or_invalid.len()).is_none(),
                "unexpected completion for {completed_or_invalid:?}"
            );
        }

        let subgraph = statement
            .suggestions
            .iter()
            .find(|suggestion| suggestion.label == "Subgraph")
            .unwrap();
        let (with_subgraph, _) = apply_mermaid_assistance(blank, subgraph).unwrap();
        assert!(parse_mermaid(&with_subgraph).is_ok());

        let existing_group = "flowchart TD\nsubgraph Group[\"Existing\"]\nend\n\n";
        let statement = mermaid_assistance_at(existing_group, existing_group.len()).unwrap();
        let subgraph = statement
            .suggestions
            .iter()
            .find(|suggestion| suggestion.label == "Subgraph")
            .unwrap();
        assert!(subgraph.insertion.starts_with("subgraph Group2["));
        let (with_second, _) = apply_mermaid_assistance(existing_group, subgraph).unwrap();
        assert!(parse_mermaid(&with_second).is_ok());

        let existing_close = "flowchart TD\nsubgraph Group[\"Existing\"]\n\nend\n";
        let empty_line = "flowchart TD\nsubgraph Group[\"Existing\"]\n".len();
        let statement = mermaid_assistance_at(existing_close, empty_line).unwrap();
        assert!(
            !statement
                .suggestions
                .iter()
                .any(|suggestion| suggestion.label == "End subgraph")
        );
    }

    #[test]
    fn mermaid_assistance_handles_semicolons_and_later_chain_targets() {
        let source = "%% note\r\ngraph RL; Beta[\"東京; today\"]; Start --> Other --> Be";
        let assistance = mermaid_assistance_at(source, source.len()).unwrap();
        assert_eq!(assistance.context, MermaidAssistanceContext::EdgeTarget);
        let beta = assistance
            .suggestions
            .iter()
            .find(|item| item.insertion == "Beta")
            .unwrap();
        let (updated, _) = apply_mermaid_assistance(source, beta).unwrap();
        assert_eq!(
            updated,
            "%% note\r\ngraph RL; Beta[\"東京; today\"]; Start --> Other --> Beta"
        );
        assert!(parse_mermaid(&updated).is_ok());

        let source = "graph TD; A; ";
        let assistance = mermaid_assistance_at(source, source.len()).unwrap();
        assert_eq!(assistance.context, MermaidAssistanceContext::Statement);

        let source = "graph\tRL; subgraph G[\"Group\"]; ";
        let assistance = mermaid_assistance_at(source, source.len()).unwrap();
        assert!(
            assistance
                .suggestions
                .iter()
                .any(|item| item.label == "End subgraph")
        );
        assert!(
            !assistance
                .suggestions
                .iter()
                .any(|item| item.label == "Subgraph")
        );

        let source = "graph  TD; subgraph G[\"Group\"]; ; end";
        let cursor = source.find("; ;").unwrap() + 2;
        let assistance = mermaid_assistance_at(source, cursor).unwrap();
        assert!(
            !assistance
                .suggestions
                .iter()
                .any(|item| item.label == "End subgraph")
        );
    }

    #[test]
    fn mermaid_assistance_does_not_edit_comments_labels_or_unsupported_families() {
        for (source, cursor) in [
            ("flowchart TD\n%% note\n", "flowchart TD\n%%".len()),
            (
                "flowchart TD\nA[\"Label\"]\n",
                "flowchart TD\nA[\"Lab".len(),
            ),
            ("sequenceDiagram\n", "sequenceDiagram\n".len()),
        ] {
            assert!(
                mermaid_assistance_at(source, cursor).is_none(),
                "{source:?}"
            );
        }
    }

    #[test]
    fn mermaid_assistance_replacement_preserves_unicode_and_crlf_surroundings() {
        let prefix = "flowchart TD\r\nA[\"東京\"]\r\n";
        let source = format!("{prefix}\r\nTail[\"最後\"]\r\n");
        let assistance = mermaid_assistance_at(&source, prefix.len()).unwrap();
        let node = assistance
            .suggestions
            .iter()
            .find(|suggestion| suggestion.label.starts_with("Declare"))
            .unwrap();
        let (updated, cursor) = apply_mermaid_assistance(&source, node).unwrap();
        assert!(updated.starts_with(prefix));
        assert!(updated.ends_with("\r\nTail[\"最後\"]\r\n"));
        assert!(updated.is_char_boundary(cursor));
        assert!(parse_mermaid(&updated).is_ok());
    }

    #[test]
    fn mermaid_assistance_never_suggests_parser_limit_breaking_edits() {
        use crate::diagram::{MAX_MERMAID_LINE_BYTES, MAX_MERMAID_SOURCE_LINES};

        let long_line = format!("flowchart TD\n{}", " ".repeat(MAX_MERMAID_LINE_BYTES));
        assert!(mermaid_assistance_at(&long_line, long_line.len()).is_none());

        let at_line_limit = format!(
            "flowchart TD\n{}",
            "\n".repeat(MAX_MERMAID_SOURCE_LINES - 1)
        );
        assert!(mermaid_assistance_at(&at_line_limit, at_line_limit.len()).is_none());
    }
}
