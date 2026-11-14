//! Safe parser and canonical formatter for the documented Mermaid subset.

use super::model::{
    DiagramDocument, DiagramEdge, DiagramGroup, DiagramNode, EdgeId, FlowDirection,
    GraphValidationError, GroupId, MAX_DIAGRAM_EDGES, MAX_DIAGRAM_GROUPS, MAX_DIAGRAM_NODES,
    MAX_GROUP_ID_BYTES, MAX_LABEL_BYTES, MAX_NODE_ID_BYTES, NodeId, NodeShape, Position,
};
use super::style::{MermaidColor, MermaidNodeStyle, extract_style};
use std::collections::HashMap;
use std::fmt;

pub const MAX_MERMAID_SOURCE_BYTES: usize = 64 * 1024;
pub const MAX_MERMAID_SOURCE_LINES: usize = 4_096;
pub const MAX_MERMAID_LINE_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MermaidDiagnosticKind {
    Invalid,
    Unsupported,
    LimitExceeded,
}

/// UTF-8-safe source range; offsets are bytes and column is one-based Unicode scalars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MermaidDiagnostic {
    pub kind: MermaidDiagnosticKind,
    pub message: String,
    pub span: SourceSpan,
}

impl fmt::Display for MermaidDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at line {}, column {}",
            self.message, self.span.line, self.span.column
        )
    }
}

impl std::error::Error for MermaidDiagnostic {}

#[derive(Debug)]
pub enum MermaidFormatError {
    InvalidGraph(GraphValidationError),
    StyleSerialization(serde_json::Error),
    SourceTooLarge { max_bytes: usize },
    LineTooLarge { max_bytes: usize },
    TooManyLines { max_lines: usize },
}

impl fmt::Display for MermaidFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGraph(error) => error.fmt(formatter),
            Self::StyleSerialization(error) => {
                write!(formatter, "could not format Mermaid style: {error}")
            }
            Self::SourceTooLarge { max_bytes } => {
                write!(
                    formatter,
                    "formatted Mermaid source exceeds {max_bytes} bytes"
                )
            }
            Self::LineTooLarge { max_bytes } => {
                write!(
                    formatter,
                    "formatted Mermaid line exceeds {max_bytes} bytes"
                )
            }
            Self::TooManyLines { max_lines } => {
                write!(
                    formatter,
                    "formatted Mermaid source exceeds {max_lines} lines"
                )
            }
        }
    }
}

impl std::error::Error for MermaidFormatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidGraph(error) => Some(error),
            Self::StyleSerialization(error) => Some(error),
            Self::SourceTooLarge { .. } | Self::LineTooLarge { .. } | Self::TooManyLines { .. } => {
                None
            }
        }
    }
}

/// A source slice with its physical location. Comments retain their original
/// bytes so visual transactions can associate them with nearby statements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourcePieceKind {
    Statement,
    Comment,
    Directive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourcePiece {
    pub kind: SourcePieceKind,
    pub range: std::ops::Range<usize>,
    pub line: usize,
    pub column_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum StatementAnchor {
    Header,
    Directive,
    Node(NodeId),
    GroupStart(GroupId),
    GroupEnd(GroupId),
    Edges(Vec<EdgeId>),
    Style(NodeId),
}

#[derive(Clone, Debug)]
pub(super) struct StatementTrace {
    pub range: std::ops::Range<usize>,
    pub anchor: StatementAnchor,
}

/// Scan bounded source without changing its bytes. Separators inside labels,
/// edge labels, shape delimiters, and Mermaid entities remain part of a token.
pub(super) fn scan_source(source: &str) -> Vec<SourcePiece> {
    let mut pieces = Vec::new();
    let mut offset = 0;
    for (index, raw) in source.split_inclusive('\n').enumerate() {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = line.trim_start();
        if trimmed.starts_with("%%") {
            pieces.push(SourcePiece {
                kind: if trimmed.starts_with("%%{") {
                    SourcePieceKind::Directive
                } else {
                    SourcePieceKind::Comment
                },
                range: offset..offset + line.len(),
                line: index + 1,
                column_offset: 0,
            });
            offset += raw.len();
            continue;
        }

        let mut start = 0;
        let mut quoted = false;
        let mut edge_label = false;
        let mut depth = 0usize;
        let mut start_column = 0;
        for (column, (pos, ch)) in line.char_indices().enumerate() {
            match ch {
                '"' if !edge_label => quoted = !quoted,
                '|' if !quoted && depth == 0 => edge_label = !edge_label,
                '[' | '(' | '{' if !quoted && !edge_label => depth += 1,
                ']' | ')' | '}' if !quoted && !edge_label => depth = depth.saturating_sub(1),
                ';' if !quoted && !edge_label && depth == 0 => {
                    pieces.push(SourcePiece {
                        kind: SourcePieceKind::Statement,
                        range: offset + start..offset + pos,
                        line: index + 1,
                        column_offset: start_column,
                    });
                    start = pos + 1;
                    start_column = column + 1;
                }
                _ => {}
            }
        }
        pieces.push(SourcePiece {
            kind: SourcePieceKind::Statement,
            range: offset + start..offset + line.len(),
            line: index + 1,
            column_offset: start_column,
        });
        offset += raw.len();
    }
    pieces
}

/// Parse the entire source or return one precise invalid/unsupported diagnostic.
///
/// A blank or comment-only draft is a valid empty top-down graph. Other source
/// begins with a supported flowchart header.
pub fn parse_mermaid(source: &str) -> Result<DiagramDocument, MermaidDiagnostic> {
    parse_mermaid_internal(source, false).map(|(graph, _)| graph)
}

pub(super) fn parse_mermaid_with_trace(
    source: &str,
) -> Result<(DiagramDocument, Vec<StatementTrace>), MermaidDiagnostic> {
    parse_mermaid_internal(source, true)
}

fn parse_mermaid_internal(
    source: &str,
    collect_trace: bool,
) -> Result<(DiagramDocument, Vec<StatementTrace>), MermaidDiagnostic> {
    preflight_source(source)?;
    if source.trim().is_empty() {
        return Ok((DiagramDocument::default(), Vec::new()));
    }

    let (style, directive) = extract_style(source).map_err(|(offset, message)| {
        let line_number = source[..offset]
            .bytes()
            .filter(|&byte| byte == b'\n')
            .count()
            + 1;
        let line = source[offset..].lines().next().unwrap_or("");
        let leading = line.len() - line.trim_start().len();
        diagnostic(
            MermaidDiagnosticKind::Unsupported,
            message,
            line,
            offset,
            line_number,
            leading,
            line.len(),
        )
    })?;
    let mut graph = DiagramDocument::default();
    graph.style = style;
    let mut pending_styles = Vec::new();
    let mut nodes_by_id = HashMap::<NodeId, usize>::new();
    let mut explicit_declarations = HashMap::<NodeId, (NodeShape, String)>::new();
    let mut active_group: Option<GroupId> = None;
    let mut active_group_span = None;
    let mut saw_header = false;
    let mut trace = Vec::new();
    for piece in scan_source(source) {
        let line_number = piece.line;
        let byte_offset = piece.range.start;
        let line = &source[piece.range.clone()];
        let trimmed = line.trim();
        if piece.kind == SourcePieceKind::Comment {
            continue;
        }
        if directive
            .as_ref()
            .is_some_and(|range| range.start == byte_offset)
        {
            if collect_trace {
                trace.push(StatementTrace {
                    range: piece.range.clone(),
                    anchor: StatementAnchor::Directive,
                });
            }
            continue;
        }
        let result = (|| -> Result<(), MermaidDiagnostic> {
            if !trimmed.is_empty() {
                if !saw_header {
                    graph.direction = parse_header(line, byte_offset, line_number)?;
                    saw_header = true;
                    if collect_trace {
                        trace.push(StatementTrace {
                            range: piece.range.clone(),
                            anchor: StatementAnchor::Header,
                        });
                    }
                } else {
                    let leading = line.len() - line.trim_start().len();
                    if trimmed.starts_with("%%") {
                        return Err(diagnostic(
                            MermaidDiagnosticKind::Unsupported,
                            "unsupported Mermaid directive",
                            line,
                            byte_offset,
                            line_number,
                            leading,
                            line.len(),
                        ));
                    }
                    if trimmed.starts_with("style ") {
                        let (id, style) = parse_node_style(trimmed).map_err(|message| {
                            diagnostic(
                                MermaidDiagnosticKind::Unsupported,
                                message,
                                line,
                                byte_offset,
                                line_number,
                                leading,
                                line.len(),
                            )
                        })?;
                        if collect_trace {
                            trace.push(StatementTrace {
                                range: piece.range.clone(),
                                anchor: StatementAnchor::Style(id.clone()),
                            });
                        }
                        pending_styles.push((
                            id,
                            style,
                            line.to_owned(),
                            byte_offset,
                            line_number,
                            leading,
                            piece.column_offset,
                        ));
                    } else if trimmed == "end" {
                        let Some(group) = active_group.take() else {
                            return Err(diagnostic(
                                MermaidDiagnosticKind::Invalid,
                                "`end` does not have an open subgraph",
                                line,
                                byte_offset,
                                line_number,
                                leading,
                                line.len(),
                            ));
                        };
                        if collect_trace {
                            trace.push(StatementTrace {
                                range: piece.range.clone(),
                                anchor: StatementAnchor::GroupEnd(group),
                            });
                        }
                        active_group_span = None;
                    } else if is_subgraph_keyword(trimmed) {
                        if active_group.is_some() {
                            return Err(diagnostic(
                                MermaidDiagnosticKind::Unsupported,
                                "nested subgraphs are outside the supported flat-group subset",
                                line,
                                byte_offset,
                                line_number,
                                leading,
                                line.len(),
                            ));
                        }
                        let (group, label) = parse_subgraph(line, byte_offset, line_number)?;
                        if graph.groups.len() >= MAX_DIAGRAM_GROUPS {
                            return Err(diagnostic(
                                MermaidDiagnosticKind::LimitExceeded,
                                format!("diagram exceeds the {MAX_DIAGRAM_GROUPS}-group limit"),
                                line,
                                byte_offset,
                                line_number,
                                leading,
                                line.len(),
                            ));
                        }
                        if graph.groups.iter().any(|existing| existing.id == group) {
                            return Err(diagnostic(
                                MermaidDiagnosticKind::Invalid,
                                format!("duplicate subgraph ID {group}"),
                                line,
                                byte_offset,
                                line_number,
                                leading,
                                line.len(),
                            ));
                        }
                        graph.groups.push(DiagramGroup {
                            id: group.clone(),
                            label,
                        });
                        if collect_trace {
                            trace.push(StatementTrace {
                                range: piece.range.clone(),
                                anchor: StatementAnchor::GroupStart(group.clone()),
                            });
                        }
                        active_group = Some(group);
                        active_group_span = Some((
                            line.to_owned(),
                            byte_offset,
                            line_number,
                            leading,
                            piece.column_offset,
                        ));
                    } else {
                        let edge_start = graph.edges.len();
                        let first = parse_statement(
                            line,
                            byte_offset,
                            line_number,
                            active_group.as_ref(),
                            &mut graph,
                            &mut nodes_by_id,
                            &mut explicit_declarations,
                        )?;
                        if collect_trace {
                            let anchor = if edge_start == graph.edges.len() {
                                StatementAnchor::Node(first)
                            } else {
                                StatementAnchor::Edges(
                                    graph.edges[edge_start..]
                                        .iter()
                                        .map(|edge| edge.id)
                                        .collect(),
                                )
                            };
                            trace.push(StatementTrace {
                                range: piece.range.clone(),
                                anchor,
                            });
                        }
                    }
                }
            }
            Ok(())
        })();
        result.map_err(|mut error| {
            error.span.column += piece.column_offset;
            error
        })?;
    }

    if !saw_header {
        if directive.is_none()
            && source.lines().all(|line| {
                let trimmed = line.trim();
                trimmed.is_empty() || (trimmed.starts_with("%%") && !trimmed.starts_with("%%{"))
            })
        {
            return Ok((graph, trace));
        }
        return Err(diagnostic(
            MermaidDiagnosticKind::Invalid,
            "expected a flowchart or graph header with TD, TB, LR, RL, or BT direction",
            source,
            0,
            1,
            0,
            source.chars().next().map_or(0, char::len_utf8),
        ));
    }
    if let Some((line, offset, number, column, column_offset)) = active_group_span {
        let mut error = diagnostic(
            MermaidDiagnosticKind::Invalid,
            "subgraph is missing its closing `end`",
            &line,
            offset,
            number,
            column,
            line.len(),
        );
        error.span.column += column_offset;
        return Err(error);
    }
    for (id, style, line, offset, number, leading, column_offset) in pending_styles {
        if !nodes_by_id.contains_key(&id) {
            let mut error = diagnostic(
                MermaidDiagnosticKind::Invalid,
                format!("style target {id} does not exist"),
                &line,
                offset,
                number,
                leading,
                line.len(),
            );
            error.span.column += column_offset;
            return Err(error);
        }
        if graph.node_styles.insert(id.clone(), style).is_some() {
            let mut error = diagnostic(
                MermaidDiagnosticKind::Invalid,
                format!("duplicate style for {id}"),
                &line,
                offset,
                number,
                leading,
                line.len(),
            );
            error.span.column += column_offset;
            return Err(error);
        }
    }
    graph.validate().map_err(|error| {
        diagnostic(
            MermaidDiagnosticKind::Invalid,
            error.to_string(),
            source,
            0,
            1,
            0,
            source.chars().next().map_or(0, char::len_utf8),
        )
    })?;
    measure_formatted_size(&graph).map_err(|error| {
        diagnostic(
            MermaidDiagnosticKind::LimitExceeded,
            format!("canonical Mermaid form exceeds subset bounds: {error}"),
            source,
            0,
            1,
            0,
            source.len(),
        )
    })?;
    Ok((graph, trace))
}

/// Whether a successfully parsed draft uses syntax older session readers rejected.
pub(super) fn uses_expanded_flowchart_syntax(source: &str) -> bool {
    let Ok((_, trace)) = parse_mermaid_with_trace(source) else {
        return false;
    };
    let pieces = scan_source(source);
    pieces.iter().any(|piece| {
        piece.kind == SourcePieceKind::Comment
            || (piece.kind == SourcePieceKind::Statement
                && (piece.column_offset > 0
                    || source.as_bytes().get(piece.range.end) == Some(&b';')))
    }) || trace.iter().any(|statement| match &statement.anchor {
        StatementAnchor::Header => {
            let header = source[statement.range.clone()].trim();
            let mut words = header.split_whitespace();
            matches!(words.next(), Some("graph")) || matches!(words.next(), Some("RL" | "BT"))
        }
        StatementAnchor::Edges(ids) => ids.len() > 1,
        _ => false,
    })
}

/// Emit normalized supported Mermaid syntax. The input graph is never mutated.
pub fn format_mermaid(graph: &DiagramDocument) -> Result<String, MermaidFormatError> {
    format_mermaid_internal(graph, false).map(|(source, _)| source)
}

pub(super) fn format_mermaid_with_anchors(
    graph: &DiagramDocument,
) -> Result<(String, HashMap<StatementAnchor, usize>), MermaidFormatError> {
    format_mermaid_internal(graph, true)
}

fn format_mermaid_internal(
    graph: &DiagramDocument,
    collect_anchors: bool,
) -> Result<(String, HashMap<StatementAnchor, usize>), MermaidFormatError> {
    graph.validate().map_err(MermaidFormatError::InvalidGraph)?;
    let encoded_bytes = measure_formatted_size(graph)?;

    let mut output = String::with_capacity(encoded_bytes);
    let mut anchors = HashMap::new();
    if !graph.style.is_empty() {
        if collect_anchors {
            anchors.insert(StatementAnchor::Directive, output.len());
        }
        output.push_str("%%{init: ");
        output.push_str(
            &serde_json::to_string(&graph.style).map_err(MermaidFormatError::StyleSerialization)?,
        );
        output.push_str("}%%\n");
    }
    if collect_anchors {
        anchors.insert(StatementAnchor::Header, output.len());
    }
    output.push_str("flowchart ");
    output.push_str(graph.direction.as_mermaid());
    output.push('\n');

    let mut emitted_groups = Vec::<GroupId>::with_capacity(graph.groups.len());
    for node in &graph.nodes {
        match &node.group {
            None => {
                if collect_anchors {
                    anchors.insert(StatementAnchor::Node(node.id.clone()), output.len());
                }
                write_node_line(&mut output, node)
            }
            Some(group_id) if !emitted_groups.contains(group_id) => {
                if let Some(group) = graph.groups.iter().find(|group| &group.id == group_id) {
                    emitted_groups.push(group_id.clone());
                    if collect_anchors {
                        anchors.insert(StatementAnchor::GroupStart(group.id.clone()), output.len());
                    }
                    output.push_str("subgraph ");
                    output.push_str(group.id.as_str());
                    output.push_str("[\"");
                    encode_mermaid_text(&group.label, &mut output);
                    output.push_str("\"]\n");
                    for member in graph
                        .nodes
                        .iter()
                        .filter(|candidate| candidate.group.as_ref() == Some(group_id))
                    {
                        if collect_anchors {
                            anchors.insert(StatementAnchor::Node(member.id.clone()), output.len());
                        }
                        write_node_line(&mut output, member);
                    }
                    if collect_anchors {
                        anchors.insert(StatementAnchor::GroupEnd(group.id.clone()), output.len());
                    }
                    output.push_str("end\n");
                }
            }
            Some(_) => {}
        }
    }
    for group in &graph.groups {
        if !emitted_groups.contains(&group.id) {
            if collect_anchors {
                anchors.insert(StatementAnchor::GroupStart(group.id.clone()), output.len());
            }
            output.push_str("subgraph ");
            output.push_str(group.id.as_str());
            output.push_str("[\"");
            encode_mermaid_text(&group.label, &mut output);
            output.push_str("\"]\n");
            if collect_anchors {
                anchors.insert(StatementAnchor::GroupEnd(group.id.clone()), output.len());
            }
            output.push_str("end\n");
        }
    }
    for edge in &graph.edges {
        if collect_anchors {
            anchors.insert(StatementAnchor::Edges(vec![edge.id]), output.len());
        }
        output.push_str(edge.from.as_str());
        output.push_str(" -->");
        if let Some(label) = &edge.label {
            output.push('|');
            encode_mermaid_text(label, &mut output);
            output.push('|');
        }
        output.push(' ');
        output.push_str(edge.to.as_str());
        output.push('\n');
    }
    for (id, style) in &graph.node_styles {
        if collect_anchors {
            anchors.insert(StatementAnchor::Style(id.clone()), output.len());
        }
        output.push_str(&format_node_style(id, style));
        output.push('\n');
    }
    Ok((output, anchors))
}

fn parse_node_style(line: &str) -> Result<(NodeId, MermaidNodeStyle), String> {
    let rest = line
        .strip_prefix("style ")
        .ok_or("expected style")?
        .trim_start();
    let (id, properties) = rest
        .split_once(char::is_whitespace)
        .ok_or("style requires a target and colors")?;
    let id = NodeId::new(id).map_err(|_| "invalid style target")?;
    let mut style = MermaidNodeStyle::default();
    for property in properties.split(',') {
        let (name, value) = property
            .trim()
            .split_once(':')
            .ok_or("expected color property")?;
        let color = MermaidColor::parse(value.trim()).map_err(str::to_owned)?;
        let slot = match name.trim() {
            "fill" => &mut style.fill,
            "stroke" => &mut style.stroke,
            "color" => &mut style.color,
            _ => return Err("unsupported style property".into()),
        };
        if slot.replace(color).is_some() {
            return Err("duplicate style property".into());
        }
    }
    if style.is_empty() {
        return Err("style requires at least one color".into());
    }
    Ok((id, style))
}

fn format_node_style(id: &NodeId, style: &MermaidNodeStyle) -> String {
    let mut properties = Vec::new();
    for (name, color) in [
        ("fill", style.fill),
        ("stroke", style.stroke),
        ("color", style.color),
    ] {
        if let Some(color) = color {
            properties.push(format!("{name}:{}", color.hex()));
        }
    }
    format!("style {id} {}", properties.join(","))
}

fn write_node_line(output: &mut String, node: &DiagramNode) {
    output.push_str(node.id.as_str());
    match node.shape {
        NodeShape::Rectangle => output.push_str("[\""),
        NodeShape::Rounded => output.push_str("(\""),
        NodeShape::Diamond => output.push_str("{\""),
    }
    encode_mermaid_text(&node.label, output);
    match node.shape {
        NodeShape::Rectangle => output.push_str("\"]\n"),
        NodeShape::Rounded => output.push_str("\")\n"),
        NodeShape::Diamond => output.push_str("\"}\n"),
    }
}

fn measure_formatted_size(graph: &DiagramDocument) -> Result<usize, MermaidFormatError> {
    let mut total_bytes = 0usize;
    let mut line_count = 0usize;
    if !graph.style.is_empty() {
        let line = format!(
            "%%{{init: {}}}%%",
            serde_json::to_string(&graph.style).map_err(MermaidFormatError::StyleSerialization)?
        );
        record_formatted_line(&mut total_bytes, &mut line_count, line.len())?;
    }
    record_formatted_line(&mut total_bytes, &mut line_count, "flowchart TD".len())?;
    for (id, style) in &graph.node_styles {
        record_formatted_line(
            &mut total_bytes,
            &mut line_count,
            format_node_style(id, style).len(),
        )?;
    }

    for node in &graph.nodes {
        let label_bytes = encoded_mermaid_text_len(&node.label)?;
        let line_bytes = node
            .id
            .as_str()
            .len()
            .checked_add(4)
            .and_then(|size| size.checked_add(label_bytes))
            .ok_or(MermaidFormatError::SourceTooLarge {
                max_bytes: MAX_MERMAID_SOURCE_BYTES,
            })?;
        record_formatted_line(&mut total_bytes, &mut line_count, line_bytes)?;
    }

    for group in &graph.groups {
        let label_bytes = encoded_mermaid_text_len(&group.label)?;
        let line_bytes = 13usize
            .checked_add(group.id.as_str().len())
            .and_then(|size| size.checked_add(label_bytes))
            .ok_or(MermaidFormatError::SourceTooLarge {
                max_bytes: MAX_MERMAID_SOURCE_BYTES,
            })?;
        record_formatted_line(&mut total_bytes, &mut line_count, line_bytes)?;
        record_formatted_line(&mut total_bytes, &mut line_count, "end".len())?;
    }

    for edge in &graph.edges {
        let mut line_bytes = edge
            .from
            .as_str()
            .len()
            .checked_add(5)
            .and_then(|size| size.checked_add(edge.to.as_str().len()))
            .ok_or(MermaidFormatError::SourceTooLarge {
                max_bytes: MAX_MERMAID_SOURCE_BYTES,
            })?;
        if let Some(label) = &edge.label {
            let label_bytes = encoded_mermaid_text_len(label)?;
            line_bytes = line_bytes
                .checked_add(2)
                .and_then(|size| size.checked_add(label_bytes))
                .ok_or(MermaidFormatError::SourceTooLarge {
                    max_bytes: MAX_MERMAID_SOURCE_BYTES,
                })?;
        }
        record_formatted_line(&mut total_bytes, &mut line_count, line_bytes)?;
    }

    Ok(total_bytes)
}

fn record_formatted_line(
    total_bytes: &mut usize,
    line_count: &mut usize,
    line_bytes: usize,
) -> Result<(), MermaidFormatError> {
    if line_bytes > MAX_MERMAID_LINE_BYTES {
        return Err(MermaidFormatError::LineTooLarge {
            max_bytes: MAX_MERMAID_LINE_BYTES,
        });
    }
    let line_with_newline =
        line_bytes
            .checked_add(1)
            .ok_or(MermaidFormatError::SourceTooLarge {
                max_bytes: MAX_MERMAID_SOURCE_BYTES,
            })?;
    *total_bytes =
        total_bytes
            .checked_add(line_with_newline)
            .ok_or(MermaidFormatError::SourceTooLarge {
                max_bytes: MAX_MERMAID_SOURCE_BYTES,
            })?;
    if *total_bytes > MAX_MERMAID_SOURCE_BYTES {
        return Err(MermaidFormatError::SourceTooLarge {
            max_bytes: MAX_MERMAID_SOURCE_BYTES,
        });
    }
    *line_count = line_count
        .checked_add(1)
        .ok_or(MermaidFormatError::TooManyLines {
            max_lines: MAX_MERMAID_SOURCE_LINES,
        })?;
    if *line_count > MAX_MERMAID_SOURCE_LINES {
        return Err(MermaidFormatError::TooManyLines {
            max_lines: MAX_MERMAID_SOURCE_LINES,
        });
    }
    Ok(())
}

fn encoded_mermaid_text_len(text: &str) -> Result<usize, MermaidFormatError> {
    text.chars().try_fold(0usize, |size, character| {
        let encoded_len = mermaid_entity(character).map_or(character.len_utf8(), str::len);
        size.checked_add(encoded_len)
            .ok_or(MermaidFormatError::SourceTooLarge {
                max_bytes: MAX_MERMAID_SOURCE_BYTES,
            })
    })
}

fn mermaid_entity(character: char) -> Option<&'static str> {
    match character {
        '#' => Some("#35;"),
        '"' => Some("#quot;"),
        '&' => Some("#38;"),
        '<' => Some("#60;"),
        '>' => Some("#62;"),
        '|' => Some("#124;"),
        '\n' => Some("#10;"),
        '\r' => Some("#13;"),
        _ => None,
    }
}

fn encode_mermaid_text(text: &str, output: &mut String) {
    for character in text.chars() {
        if let Some(entity) = mermaid_entity(character) {
            output.push_str(entity);
        } else {
            output.push(character);
        }
    }
}

pub(super) fn preflight_source(source: &str) -> Result<(), MermaidDiagnostic> {
    if source.len() > MAX_MERMAID_SOURCE_BYTES {
        return Err(diagnostic(
            MermaidDiagnosticKind::LimitExceeded,
            format!("source exceeds the {MAX_MERMAID_SOURCE_BYTES}-byte limit"),
            source,
            0,
            1,
            0,
            source.len(),
        ));
    }
    let newline_count = source.bytes().filter(|byte| *byte == b'\n').count();
    let line_count = newline_count + usize::from(!source.is_empty() && !source.ends_with('\n'));
    if line_count > MAX_MERMAID_SOURCE_LINES {
        let line_start = source
            .match_indices('\n')
            .nth(MAX_MERMAID_SOURCE_LINES - 1)
            .map_or(0, |(offset, newline)| offset + newline.len());
        let offending_line = source[line_start..]
            .split_once('\n')
            .map_or(&source[line_start..], |(line, _)| line);
        return Err(diagnostic(
            MermaidDiagnosticKind::LimitExceeded,
            format!("source exceeds the {MAX_MERMAID_SOURCE_LINES}-line limit"),
            offending_line,
            line_start,
            MAX_MERMAID_SOURCE_LINES + 1,
            0,
            offending_line.len(),
        ));
    }
    let mut byte_offset = 0usize;
    for (line_index, line_with_newline) in source.split_inclusive('\n').enumerate() {
        let line_with_cr = line_with_newline
            .strip_suffix('\n')
            .unwrap_or(line_with_newline);
        let line = line_with_cr.strip_suffix('\r').unwrap_or(line_with_cr);
        if line.len() > MAX_MERMAID_LINE_BYTES {
            let start = line
                .char_indices()
                .find_map(|(offset, _)| (offset >= MAX_MERMAID_LINE_BYTES).then_some(offset))
                .unwrap_or(line.len());
            return Err(diagnostic(
                MermaidDiagnosticKind::LimitExceeded,
                format!("line exceeds the {MAX_MERMAID_LINE_BYTES}-byte limit"),
                line,
                byte_offset,
                line_index + 1,
                start,
                line.len(),
            ));
        }
        byte_offset = byte_offset.saturating_add(line_with_newline.len());
    }
    Ok(())
}

pub(super) fn parse_header(
    line: &str,
    byte_offset: usize,
    line_number: usize,
) -> Result<FlowDirection, MermaidDiagnostic> {
    let leading = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    let Some(after_keyword) = trimmed
        .strip_prefix("flowchart")
        .or_else(|| trimmed.strip_prefix("graph"))
    else {
        return Err(diagnostic(
            MermaidDiagnosticKind::Unsupported,
            "only Mermaid flowcharts are supported",
            line,
            byte_offset,
            line_number,
            leading,
            line.len(),
        ));
    };
    if after_keyword
        .chars()
        .next()
        .is_none_or(|character| !character.is_whitespace())
    {
        return Err(diagnostic(
            MermaidDiagnosticKind::Invalid,
            "flowchart header requires a direction",
            line,
            byte_offset,
            line_number,
            leading,
            line.len(),
        ));
    }
    match after_keyword.trim() {
        "TD" => Ok(FlowDirection::TopDown),
        "TB" => Ok(FlowDirection::TopBottom),
        "LR" => Ok(FlowDirection::LeftRight),
        "RL" => Ok(FlowDirection::RightLeft),
        "BT" => Ok(FlowDirection::BottomTop),
        other => Err(diagnostic(
            MermaidDiagnosticKind::Invalid,
            format!("unknown flowchart direction {other:?}"),
            line,
            byte_offset,
            line_number,
            leading,
            line.len(),
        )),
    }
}

fn parse_subgraph(
    line: &str,
    byte_offset: usize,
    line_number: usize,
) -> Result<(GroupId, String), MermaidDiagnostic> {
    let mut parser = StatementParser::new(line, byte_offset, line_number);
    parser.skip_whitespace();
    parser.consume("subgraph");
    parser.require_whitespace("expected an explicit subgraph ID")?;
    let id_offset = parser.position;
    let raw_id = parser.parse_identifier(MAX_GROUP_ID_BYTES)?;
    let group = GroupId::new(raw_id).map_err(|error| parser.graph_error(error, id_offset))?;
    parser.skip_whitespace();
    let (shape, label) = parser.parse_shape_label()?;
    if shape != NodeShape::Rectangle {
        return Err(parser.invalid("subgraph labels use square brackets", id_offset));
    }
    parser.finish()?;
    Ok((group, label))
}

/// Return bounded grammar-recognized IDs and edge count for completion UI.
/// This scans individual supported statements without building a graph or
/// running the renderer on every editor frame.
pub(super) fn assistance_symbols(source: &str) -> (Vec<String>, Vec<String>, usize) {
    if preflight_source(source).is_err() {
        return (Vec::new(), Vec::new(), 0);
    }
    let mut nodes = Vec::new();
    let mut seen_nodes = std::collections::HashSet::new();
    let mut groups = Vec::new();
    let mut seen_groups = std::collections::HashSet::new();
    let mut edges = 0usize;
    let mut skipped_header = false;
    for piece in scan_source(source) {
        if piece.kind != SourcePieceKind::Statement {
            continue;
        }
        let line = &source[piece.range.clone()];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !skipped_header {
            skipped_header = true;
            continue;
        }
        if trimmed == "end" {
            continue;
        }
        if is_subgraph_keyword(trimmed) {
            if let Ok((group, _)) = parse_subgraph(line, piece.range.start, piece.line)
                && seen_groups.insert(group.as_str().to_owned())
            {
                groups.push(group.as_str().to_owned());
            }
        } else {
            let mut parser = StatementParser::new(line, piece.range.start, piece.line);
            parser.skip_whitespace();
            let Ok(first) = parser.parse_node_term() else {
                continue;
            };
            let mut line_nodes = vec![first.id.as_str().to_owned()];
            let mut line_edges = 0usize;
            parser.skip_whitespace();
            let mut valid = true;
            while parser.remaining().starts_with("-->") {
                parser.position += 3;
                parser.skip_whitespace();
                if parser.remaining().starts_with('|') && parser.parse_edge_label().is_err() {
                    valid = false;
                    break;
                }
                parser.skip_whitespace();
                match parser.parse_node_term() {
                    Ok(node) => line_nodes.push(node.id.as_str().to_owned()),
                    Err(_) => {
                        valid = false;
                        break;
                    }
                }
                line_edges += 1;
                parser.skip_whitespace();
            }
            if valid && parser.finish().is_ok() {
                for id in line_nodes {
                    if seen_nodes.insert(id.clone()) {
                        nodes.push(id);
                    }
                }
                edges = edges.saturating_add(line_edges).min(MAX_DIAGRAM_EDGES);
            }
        }
        if nodes.len() >= MAX_DIAGRAM_NODES
            || groups.len() >= MAX_DIAGRAM_GROUPS
            || edges >= MAX_DIAGRAM_EDGES
        {
            break;
        }
    }
    (nodes, groups, edges)
}

/// Locate the target identifier of any supported arrow in a chain.
pub(super) fn assistance_edge_target_span(line: &str, cursor: usize) -> Option<(usize, usize)> {
    if line.len() > MAX_MERMAID_LINE_BYTES || cursor > line.len() || !line.is_char_boundary(cursor)
    {
        return None;
    }
    let mut parser = StatementParser::new(line, 0, 1);
    parser.skip_whitespace();
    parser.parse_node_term().ok()?;
    parser.skip_whitespace();
    while parser.remaining().starts_with("-->") {
        parser.position += 3;
        parser.skip_whitespace();
        if parser.remaining().starts_with('|') {
            parser.parse_edge_label().ok()?;
            parser.skip_whitespace();
        }
        let start = parser.position;
        if parser.remaining().is_empty() {
            return (cursor == start).then_some((start, start));
        }
        parser.parse_identifier(MAX_NODE_ID_BYTES).ok()?;
        let end = parser.position;
        parser.skip_whitespace();
        if parser
            .remaining()
            .as_bytes()
            .first()
            .is_some_and(|byte| matches!(byte, b'[' | b'(' | b'{'))
        {
            parser.parse_shape_label().ok()?;
        }
        parser.skip_whitespace();
        if start <= cursor && cursor <= end {
            if parser.is_finished() || parser.remaining().starts_with("-->") {
                return Some((start, end));
            }
            return None;
        }
    }
    None
}

pub(super) fn is_subgraph_keyword(trimmed: &str) -> bool {
    trimmed == "subgraph"
        || trimmed
            .strip_prefix("subgraph")
            .is_some_and(|rest| rest.chars().next().is_some_and(char::is_whitespace))
}

#[derive(Clone)]
struct NodeTerm {
    id: NodeId,
    declaration: Option<(NodeShape, String)>,
}

fn parse_statement(
    line: &str,
    byte_offset: usize,
    line_number: usize,
    active_group: Option<&GroupId>,
    graph: &mut DiagramDocument,
    nodes_by_id: &mut HashMap<NodeId, usize>,
    explicit_declarations: &mut HashMap<NodeId, (NodeShape, String)>,
) -> Result<NodeId, MermaidDiagnostic> {
    let mut parser = StatementParser::new(line, byte_offset, line_number);
    parser.skip_whitespace();
    let first = parser.parse_node_term()?;
    let first_id = first.id.clone();
    parser.skip_whitespace();
    let mut from = ensure_node(
        graph,
        nodes_by_id,
        explicit_declarations,
        first,
        active_group,
        &parser,
    )?;
    while parser.remaining().starts_with("-->") {
        let edge_start = parser.position;
        parser.position += 3;
        parser.skip_whitespace();
        let edge_label = if parser.remaining().starts_with('|') {
            Some(parser.parse_edge_label()?)
        } else {
            None
        };
        parser.skip_whitespace();
        let second = parser.parse_node_term()?;
        parser.skip_whitespace();
        let to = ensure_node(
            graph,
            nodes_by_id,
            explicit_declarations,
            second,
            active_group,
            &parser,
        )?;
        if graph.edges.len() >= MAX_DIAGRAM_EDGES {
            return Err(parser.error(
                MermaidDiagnosticKind::LimitExceeded,
                format!("diagram exceeds the {MAX_DIAGRAM_EDGES}-edge limit"),
                edge_start,
                line.len(),
            ));
        }
        let id = graph
            .allocate_edge_id()
            .map_err(|error| parser.graph_error(error, 0))?;
        graph.edges.push(DiagramEdge {
            id,
            from: from.clone(),
            to: to.clone(),
            label: edge_label,
        });
        from = to;
    }
    if !parser.is_finished() {
        return Err(parser.error(
            MermaidDiagnosticKind::Unsupported,
            "unsupported token or trailing syntax in flowchart statement",
            parser.position,
            line.len(),
        ));
    }
    Ok(first_id)
}

fn ensure_node(
    graph: &mut DiagramDocument,
    nodes_by_id: &mut HashMap<NodeId, usize>,
    explicit_declarations: &mut HashMap<NodeId, (NodeShape, String)>,
    term: NodeTerm,
    active_group: Option<&GroupId>,
    parser: &StatementParser<'_>,
) -> Result<NodeId, MermaidDiagnostic> {
    if let Some(index) = nodes_by_id.get(&term.id).copied() {
        if let Some((shape, label)) = term.declaration {
            match explicit_declarations.get(&term.id) {
                Some((prior_shape, prior_label))
                    if *prior_shape != shape || *prior_label != label =>
                {
                    return Err(parser.error(
                        MermaidDiagnosticKind::Invalid,
                        format!("node {} has conflicting declarations", term.id),
                        0,
                        parser.line.len(),
                    ));
                }
                Some(_) => {}
                None => {
                    let node = &mut graph.nodes[index];
                    node.shape = shape;
                    node.label = label.clone();
                    explicit_declarations.insert(term.id.clone(), (shape, label));
                }
            }
        }
        return Ok(term.id);
    }
    if graph.nodes.len() >= MAX_DIAGRAM_NODES {
        return Err(parser.error(
            MermaidDiagnosticKind::LimitExceeded,
            format!("diagram exceeds the {MAX_DIAGRAM_NODES}-node limit"),
            0,
            parser.line.len(),
        ));
    }
    let (shape, label) = term.declaration.map_or(
        (NodeShape::Rectangle, term.id.as_str().to_owned()),
        |(shape, label)| {
            explicit_declarations.insert(term.id.clone(), (shape, label.clone()));
            (shape, label)
        },
    );
    let index = graph.nodes.len();
    graph.nodes.push(DiagramNode {
        id: term.id.clone(),
        label,
        shape,
        position: Position::default(),
        group: active_group.cloned(),
    });
    nodes_by_id.insert(term.id.clone(), index);
    Ok(term.id)
}

struct StatementParser<'a> {
    line: &'a str,
    byte_offset: usize,
    line_number: usize,
    position: usize,
}

impl<'a> StatementParser<'a> {
    fn new(line: &'a str, byte_offset: usize, line_number: usize) -> Self {
        Self {
            line,
            byte_offset,
            line_number,
            position: 0,
        }
    }

    fn remaining(&self) -> &'a str {
        &self.line[self.position..]
    }

    fn is_finished(&self) -> bool {
        self.position == self.line.len()
    }

    fn skip_whitespace(&mut self) {
        while let Some(character) = self.remaining().chars().next()
            && character.is_whitespace()
        {
            self.position += character.len_utf8();
        }
    }

    fn require_whitespace(&mut self, message: &str) -> Result<(), MermaidDiagnostic> {
        let before = self.position;
        self.skip_whitespace();
        if self.position == before {
            return Err(self.invalid(message, self.position));
        }
        Ok(())
    }

    fn consume(&mut self, text: &str) {
        debug_assert!(self.remaining().starts_with(text));
        self.position += text.len();
    }

    fn parse_identifier(&mut self, max_bytes: usize) -> Result<String, MermaidDiagnostic> {
        let start = self.position;
        let bytes = self.line.as_bytes();
        if !bytes
            .get(self.position)
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
        {
            return Err(self.invalid("expected an ASCII graph identifier", start));
        }
        self.position += 1;
        while let Some(byte) = bytes.get(self.position) {
            if self.remaining().starts_with("-->") {
                break;
            }
            if *byte == b'-' {
                if bytes.get(self.position + 1) == Some(&b'-') {
                    let run_end = bytes[self.position..]
                        .iter()
                        .position(|byte| *byte != b'-')
                        .map_or(bytes.len(), |offset| self.position + offset);
                    return Err(self.error(
                        MermaidDiagnosticKind::Unsupported,
                        "repeated hyphens are reserved for Mermaid edge syntax",
                        self.position,
                        run_end,
                    ));
                }
                self.position += 1;
            } else if byte.is_ascii_alphanumeric() || *byte == b'_' {
                self.position += 1;
            } else {
                break;
            }
        }
        if self.position - start > max_bytes {
            return Err(self.error(
                MermaidDiagnosticKind::LimitExceeded,
                format!("identifier exceeds the {max_bytes}-byte limit"),
                start,
                self.position,
            ));
        }
        Ok(self.line[start..self.position].to_owned())
    }

    fn parse_node_term(&mut self) -> Result<NodeTerm, MermaidDiagnostic> {
        let start = self.position;
        let raw_id = self.parse_identifier(MAX_NODE_ID_BYTES)?;
        let id = NodeId::new(raw_id).map_err(|error| self.graph_error(error, start))?;
        self.skip_whitespace();
        let declaration = match self.remaining().as_bytes().first().copied() {
            Some(b'[') => Some(self.parse_shape_label()?),
            Some(b'(') => Some(self.parse_shape_label()?),
            Some(b'{') => Some(self.parse_shape_label()?),
            _ => None,
        };
        Ok(NodeTerm { id, declaration })
    }

    fn parse_shape_label(&mut self) -> Result<(NodeShape, String), MermaidDiagnostic> {
        let open = self.remaining().as_bytes().first().copied();
        let (shape, close) = match open {
            Some(b'[') => (NodeShape::Rectangle, b']'),
            Some(b'(') => (NodeShape::Rounded, b')'),
            Some(b'{') => (NodeShape::Diamond, b'}'),
            _ => return Err(self.invalid("expected a supported shape delimiter", self.position)),
        };
        let start = self.position;
        self.position += 1;
        self.skip_whitespace();
        let label = self.parse_quoted_text()?;
        self.skip_whitespace();
        if self.remaining().as_bytes().first().copied() != Some(close) {
            return Err(self.invalid("shape label is missing its closing delimiter", start));
        }
        self.position += 1;
        Ok((shape, label))
    }

    fn parse_quoted_text(&mut self) -> Result<String, MermaidDiagnostic> {
        if self.remaining().as_bytes().first().copied() != Some(b'"') {
            return Err(self.invalid("supported labels must be double-quoted", self.position));
        }
        let quote_start = self.position;
        self.position += 1;
        let content_start = self.position;
        let mut cursor = self.position;
        while cursor < self.line.len() {
            let character = self.line[cursor..]
                .chars()
                .next()
                .expect("cursor is at a UTF-8 character boundary");
            if character == '"' {
                let raw = &self.line[content_start..cursor];
                let decoded = decode_mermaid_entities(raw).map_err(|error| {
                    self.error(
                        MermaidDiagnosticKind::Invalid,
                        error.message,
                        content_start + error.offset,
                        content_start + error.offset + error.length,
                    )
                })?;
                if decoded.len() > MAX_LABEL_BYTES {
                    return Err(self.error(
                        MermaidDiagnosticKind::LimitExceeded,
                        format!("decoded label exceeds the {MAX_LABEL_BYTES}-byte limit"),
                        content_start,
                        cursor,
                    ));
                }
                if decoded.contains('\0') {
                    return Err(self.error(
                        MermaidDiagnosticKind::Invalid,
                        "labels may not contain a NUL character",
                        content_start,
                        cursor,
                    ));
                }
                self.position = cursor + 1;
                return Ok(decoded);
            }
            cursor += character.len_utf8();
        }
        Err(self.error(
            MermaidDiagnosticKind::Invalid,
            "quoted label is not closed",
            quote_start,
            self.line.len(),
        ))
    }

    fn parse_edge_label(&mut self) -> Result<String, MermaidDiagnostic> {
        debug_assert!(self.remaining().starts_with('|'));
        let opener = self.position;
        self.position += 1;
        let start = self.position;
        let end = self
            .remaining()
            .find('|')
            .map(|offset| self.position + offset);
        let Some(end) = end else {
            return Err(self.invalid("edge label is missing its closing `|`", opener));
        };
        let raw = &self.line[start..end];
        let label = decode_mermaid_entities(raw).map_err(|error| {
            self.error(
                MermaidDiagnosticKind::Invalid,
                error.message,
                start + error.offset,
                start + error.offset + error.length,
            )
        })?;
        if label.len() > MAX_LABEL_BYTES {
            return Err(self.error(
                MermaidDiagnosticKind::LimitExceeded,
                format!("decoded edge label exceeds the {MAX_LABEL_BYTES}-byte limit"),
                start,
                end,
            ));
        }
        if label.contains('\0') {
            return Err(self.error(
                MermaidDiagnosticKind::Invalid,
                "labels may not contain a NUL character",
                start,
                end,
            ));
        }
        self.position = end + 1;
        Ok(label)
    }

    fn finish(&mut self) -> Result<(), MermaidDiagnostic> {
        self.skip_whitespace();
        if self.is_finished() {
            Ok(())
        } else {
            Err(self.error(
                MermaidDiagnosticKind::Unsupported,
                "trailing syntax is outside the supported subset",
                self.position,
                self.line.len(),
            ))
        }
    }

    fn invalid(&self, message: impl Into<String>, position: usize) -> MermaidDiagnostic {
        let end = self
            .line
            .get(position..)
            .and_then(|remaining| remaining.chars().next())
            .map_or(position, |character| position + character.len_utf8());
        self.error(MermaidDiagnosticKind::Invalid, message, position, end)
    }

    fn graph_error(&self, error: GraphValidationError, position: usize) -> MermaidDiagnostic {
        let end = self
            .line
            .get(position..)
            .and_then(|remaining| remaining.chars().next())
            .map_or(position, |character| position + character.len_utf8());
        self.error(
            MermaidDiagnosticKind::Invalid,
            error.to_string(),
            position,
            end,
        )
    }

    fn error(
        &self,
        kind: MermaidDiagnosticKind,
        message: impl Into<String>,
        start: usize,
        end: usize,
    ) -> MermaidDiagnostic {
        diagnostic(
            kind,
            message,
            self.line,
            self.byte_offset,
            self.line_number,
            start,
            end,
        )
    }
}

#[derive(Debug)]
struct EntityDecodeError {
    offset: usize,
    length: usize,
    message: String,
}

fn decode_mermaid_entities(raw: &str) -> Result<String, EntityDecodeError> {
    let mut decoded = String::with_capacity(raw.len());
    let mut cursor = 0usize;
    while cursor < raw.len() {
        let character = raw[cursor..]
            .chars()
            .next()
            .expect("cursor is at a UTF-8 character boundary");
        if character != '#' {
            decoded.push(character);
            cursor += character.len_utf8();
            continue;
        }
        let Some(semicolon) = raw[cursor..]
            .bytes()
            .take(16)
            .position(|byte| byte == b';')
            .map(|offset| cursor + offset)
        else {
            decoded.push('#');
            cursor += 1;
            continue;
        };
        let entity = &raw[cursor + 1..semicolon];
        let entity_end = semicolon + 1;
        let replacement = match entity {
            "quot" => Some('"'),
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "apos" => Some('\''),
            _ if !entity.is_empty() && entity.bytes().all(|byte| byte.is_ascii_digit()) => {
                match entity.parse::<u32>().ok().and_then(char::from_u32) {
                    Some(character) => Some(character),
                    None => {
                        return Err(EntityDecodeError {
                            offset: cursor,
                            length: entity_end - cursor,
                            message: "numeric Mermaid entity is not a Unicode scalar value".into(),
                        });
                    }
                }
            }
            _ => None,
        };
        if let Some(character) = replacement {
            decoded.push(character);
            cursor = entity_end;
        } else if entity.is_empty() || entity.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(EntityDecodeError {
                offset: cursor,
                length: entity_end - cursor,
                message: "malformed Mermaid entity".into(),
            });
        } else {
            // A hash not introducing a recognized entity is literal source text.
            decoded.push('#');
            cursor += 1;
        }
    }
    Ok(decoded)
}

fn diagnostic(
    kind: MermaidDiagnosticKind,
    message: impl Into<String>,
    source_line: &str,
    byte_offset: usize,
    line: usize,
    start: usize,
    end: usize,
) -> MermaidDiagnostic {
    let mut start = start.min(source_line.len());
    while !source_line.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = end.max(start).min(source_line.len());
    while !source_line.is_char_boundary(end) {
        end += 1;
    }
    // All callers pass line and token boundaries produced by char iteration or
    // ASCII delimiters, so these slices remain on UTF-8 boundaries.
    let column = source_line[..start].chars().count() + 1;
    MermaidDiagnostic {
        kind,
        message: message.into(),
        span: SourceSpan {
            start_byte: byte_offset + start,
            end_byte: byte_offset + end,
            line,
            column,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_MERMAID_LINE_BYTES, MAX_MERMAID_SOURCE_BYTES, MAX_MERMAID_SOURCE_LINES,
        MermaidDiagnosticKind, MermaidFormatError, format_mermaid, parse_mermaid,
    };
    use crate::diagram::{
        DiagramDocument, DiagramNode, FlowDirection, NodeId, NodeShape, Position,
    };

    #[test]
    fn color_envelope_and_node_styles_roundtrip_and_reject_unsupported_input() {
        let source = "%%{init: {\"theme\":\"dark\",\"themeVariables\":{\"primaryColor\":\"#123456\",\"mindmapSectionColors\":[\"#abc\",\"#abcdef\"]}}}%%\r\nflowchart TD\r\nA[\"東京\"] --> B\r\nstyle A fill:#abc,stroke:#123456,color:#fedcba\r\n";
        let graph = parse_mermaid(source).unwrap();
        let canonical = format_mermaid(&graph).unwrap();
        assert_eq!(
            super::measure_formatted_size(&graph).unwrap(),
            canonical.len()
        );
        assert_eq!(parse_mermaid(&canonical).unwrap(), graph);
        for source in [
            "%%{init: {\"theme\":\"dark\",\"securityLevel\":\"loose\"}}%%\nflowchart TD\nA\n",
            "%%{init: {\"theme\":\"dark\",\"theme\":\"forest\"}}%%\nflowchart TD\nA\n",
            "flowchart TD\nA\nstyle B fill:#abc\n",
            "flowchart TD\nA\nstyle A fill:#abc,fill:#123\n",
            "flowchart TD\nA\nstyle A fill:#abc,stroke-width:2\n",
            "flowchart TD\nA\nstyle A fill:#badbadz\n",
            "flowchart TD\nA\nstyle A fill:#abc\nstyle A stroke:#123\n",
        ] {
            assert!(parse_mermaid(source).is_err(), "{source}");
        }
        let diagnostic =
            parse_mermaid("flowchart TD\nA[\"東京\"]\nstyle Missing fill:#abc\n").unwrap_err();
        assert_eq!(diagnostic.span.line, 3);
    }

    #[test]
    fn diagram_mermaid_supported_subset_roundtrips_shapes_groups_edges_and_unicode() {
        let source = concat!(
            "flowchart LR\n",
            "A[\"Start #quot; & #38; #35;quot; 🧭\"]\n",
            "subgraph Decisions[\"Pick #35;\"]\n",
            "B(\"Rounded #40; #41;\")\n",
            "C{\"Yes | no\"}\n",
            "end\n",
            "B -->|approve #124; now| C\n",
            "A --> B\n",
            "A --> B\n",
        );
        let graph = parse_mermaid(source).unwrap();
        assert_eq!(graph.direction, FlowDirection::LeftRight);
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.nodes[0].label, "Start \" & & #quot; 🧭");
        assert_eq!(graph.nodes[1].shape, NodeShape::Rounded);
        assert_eq!(graph.nodes[1].label, "Rounded ( )");
        assert_eq!(graph.nodes[2].shape, NodeShape::Diamond);
        assert_eq!(graph.nodes[2].label, "Yes | no");
        assert_eq!(graph.nodes[1].group.as_ref().unwrap().as_str(), "Decisions");
        assert_eq!(graph.groups[0].label, "Pick #");
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.edges[0].label.as_deref(), Some("approve | now"));
        assert_eq!(graph.edges[1].id.get(), 2);
        assert_eq!(graph.edges[2].id.get(), 3);
        assert_eq!(graph.next_edge_id, 4);

        let formatted = format_mermaid(&graph).unwrap();
        let reparsed = parse_mermaid(&formatted).unwrap();
        assert_eq!(reparsed, graph);
    }

    #[test]
    fn diagram_mermaid_entities_are_decoded_once_and_hashes_are_safe_to_format() {
        let graph = parse_mermaid(
            "flowchart TD\nA[\"#quot; #35;quot; #91; #93; #123; #125; #124; #38;\"]\n",
        )
        .unwrap();
        assert_eq!(graph.nodes[0].label, "\" #quot; [ ] { } | &");
        let formatted = format_mermaid(&graph).unwrap();
        assert!(formatted.contains("#quot; #35;quot;"));
        assert_eq!(parse_mermaid(&formatted).unwrap(), graph);
    }

    #[test]
    fn diagram_mermaid_accepts_crlf_and_normalizes_only_formatted_output() {
        let source = "flowchart TB\r\nA[\"one\"]\r\nA --> B\r\n";
        let graph = parse_mermaid(source).unwrap();
        assert_eq!(graph.direction, FlowDirection::TopBottom);
        let formatted = format_mermaid(&graph).unwrap();
        assert!(!formatted.contains('\r'));
        assert_eq!(parse_mermaid(&formatted).unwrap(), graph);
        assert!(parse_mermaid(" \r\n\t").unwrap().nodes.is_empty());
    }

    #[test]
    fn diagram_mermaid_allows_single_hyphens_and_rejects_ambiguous_runs() {
        let graph = parse_mermaid("flowchart TD\nA-B --> C\n").unwrap();
        assert_eq!(graph.nodes[0].id.as_str(), "A-B");

        for source in [
            "flowchart TD\nA--->B\n",
            "flowchart TD\nA---B\n",
            "flowchart TD\nA--oB\n",
            "flowchart TD\nA--xB\n",
        ] {
            assert_eq!(
                parse_mermaid(source).unwrap_err().kind,
                MermaidDiagnosticKind::Unsupported,
                "{source}"
            );
        }
    }

    #[test]
    fn diagram_mermaid_rejects_unsupported_constructs_without_prefix_acceptance() {
        for source in [
            "flowchart TD\n%%{init: {\"theme\":\"dark\"}}%%\n",
            "flowchart TD\nsubgraph Outer[\"outer\"]\nsubgraph Inner[\"inner\"]\n",
            "flowchart TD\nA <--> B\n",
            "sequenceDiagram\nAlice->>Bob: hello\n",
        ] {
            let diagnostic = parse_mermaid(source).expect_err(source);
            assert_eq!(
                diagnostic.kind,
                MermaidDiagnosticKind::Unsupported,
                "{source}"
            );
            assert!(diagnostic.span.start_byte <= source.len());
            assert!(diagnostic.span.end_byte <= source.len());
            assert!(source.is_char_boundary(diagnostic.span.start_byte));
            assert!(source.is_char_boundary(diagnostic.span.end_byte));
        }
    }

    #[test]
    fn diagram_mermaid_accepts_comments_separators_and_chained_arrows() {
        let source = concat!(
            "%% Keep this introduction\r\n",
            "graph RL; A[\"Start; ready\"] -->|yes| B[\"Work\"]; B --> C[\"Finish\"];\r\n",
            "%% Keep the finish explanation\r\n",
            "C --> D[\"Done\"]\r\n",
        );
        let graph = parse_mermaid(source).unwrap();
        assert_eq!(graph.direction, FlowDirection::RightLeft);
        assert_eq!(graph.nodes.len(), 4);
        assert_eq!(graph.nodes[0].label, "Start; ready");
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.edges[0].label.as_deref(), Some("yes"));
        assert_eq!(
            parse_mermaid(&format_mermaid(&graph).unwrap()).unwrap(),
            graph
        );

        let chain = parse_mermaid("graph BT; A -->|one; two| B -->|three| C;").unwrap();
        assert_eq!(chain.direction, FlowDirection::BottomTop);
        assert_eq!(chain.edges.len(), 2);
        assert_eq!(chain.edges[0].label.as_deref(), Some("one; two"));
        assert_eq!(chain.edges[1].label.as_deref(), Some("three"));
        assert_eq!(chain.edges[0].to, chain.edges[1].from);
        assert!(
            parse_mermaid("%% only a comment\r\n  %% second\r\n")
                .unwrap()
                .nodes
                .is_empty()
        );
        assert!(parse_mermaid(";;;").is_err());
    }

    #[test]
    fn diagram_mermaid_keeps_original_positions_after_separators() {
        let source = "%% comment\r\ngraph TD; A[\"東京; #59;\"]; A --> B --> 🧭\r\n";
        let error = parse_mermaid(source).unwrap_err();
        let expected = source.find('🧭').unwrap();
        assert_eq!(error.span.start_byte, expected);
        assert_eq!(error.span.end_byte, expected + '🧭'.len_utf8());
        assert_eq!(error.span.line, 2);
        let line = source.lines().nth(1).unwrap();
        assert_eq!(
            error.span.column,
            line[..line.find('🧭').unwrap()].chars().count() + 1
        );

        let styled = "%% first\n%%{init: {\"theme\":\"dark\"}}%%\ngraph TD; A;";
        assert!(parse_mermaid(styled).is_ok());
        assert!(parse_mermaid("%% first\n%%{unknown}%%\ngraph TD; A;").is_err());
    }

    #[test]
    fn diagram_mermaid_checks_expanded_chain_limits_and_style_separators() {
        let graph = parse_mermaid("graph TD; A[\"#59;\"]; style A fill:#abc; B;").unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.nodes[0].label, ";");
        assert_eq!(graph.node_styles.len(), 1);

        let mut source = String::from("graph TD\n");
        for _ in 0..super::MAX_DIAGRAM_EDGES - 1 {
            source.push_str("A --> B\n");
        }
        source.push_str("A --> B --> C");
        let error = parse_mermaid(&source).unwrap_err();
        assert_eq!(error.kind, MermaidDiagnosticKind::LimitExceeded);
        assert_eq!(error.span.start_byte, source.rfind("-->").unwrap());
    }

    #[test]
    fn diagram_mermaid_diagnoses_invalid_syntax_and_utf8_safe_columns() {
        for source in [
            "flowchart XD\nA --> B\n",
            "flowchart TD\nA[unquoted]\n",
            "flowchart TD\nA[\"unclosed]\n",
            "flowchart TD\nend[\"finish\"]\n",
            "flowchart TD\nA[\"bad#0;\"]\n",
            "flowchart TD\nA -->|bad\0| B\n",
            "flowchart TD\nend\n",
            "flowchart TD\nsubgraph end[\"reserved\"]\nA\nend\n",
            "flowchart TD\nsubgraph Group[\"missing end\"]\nA\n",
        ] {
            assert_eq!(
                parse_mermaid(source).unwrap_err().kind,
                MermaidDiagnosticKind::Invalid,
                "{source}"
            );
        }

        let source = "flowchart TD\n🧭 --> A\n";
        let diagnostic = parse_mermaid(source).unwrap_err();
        assert_eq!(diagnostic.kind, MermaidDiagnosticKind::Invalid);
        assert_eq!(diagnostic.span.line, 2);
        assert_eq!(diagnostic.span.column, 1);
        assert!(source.is_char_boundary(diagnostic.span.start_byte));
        assert!(source.is_char_boundary(diagnostic.span.end_byte));
        assert_eq!(
            &source[diagnostic.span.start_byte..diagnostic.span.end_byte],
            "🧭"
        );
    }

    #[test]
    fn diagram_mermaid_enforces_source_line_and_source_byte_bounds_before_parsing() {
        let too_large = "x".repeat(MAX_MERMAID_SOURCE_BYTES + 1);
        assert_eq!(
            parse_mermaid(&too_large).unwrap_err().kind,
            MermaidDiagnosticKind::LimitExceeded
        );
        let too_many_lines = "\n".repeat(MAX_MERMAID_SOURCE_LINES + 1);
        assert_eq!(
            parse_mermaid(&too_many_lines).unwrap_err().kind,
            MermaidDiagnosticKind::LimitExceeded
        );
        let long_line = format!("flowchart TD\n{}", "A".repeat(MAX_MERMAID_LINE_BYTES + 1));
        assert_eq!(
            parse_mermaid(&long_line).unwrap_err().kind,
            MermaidDiagnosticKind::LimitExceeded
        );
    }

    #[test]
    fn diagram_mermaid_rejects_conflicting_node_declarations_and_bounds_graph_counts() {
        let conflict = "flowchart TD\nA[\"first\"]\nA{\"second\"}\n";
        assert_eq!(
            parse_mermaid(conflict).unwrap_err().kind,
            MermaidDiagnosticKind::Invalid
        );

        let mut source = String::from("flowchart TD\n");
        for index in 0..=crate::diagram::MAX_DIAGRAM_NODES {
            source.push_str(&format!("N{index}\n"));
        }
        assert_eq!(
            parse_mermaid(&source).unwrap_err().kind,
            MermaidDiagnosticKind::LimitExceeded
        );

        let colliding_namespace = "flowchart TD\nsubgraph G[\"group\"]\nA\nend\nG[\"node\"]\n";
        assert_eq!(
            parse_mermaid(colliding_namespace).unwrap_err().kind,
            MermaidDiagnosticKind::Invalid
        );
    }

    #[test]
    fn diagram_mermaid_uses_decoded_label_limits_and_exact_formatted_sizes() {
        let literal_hash_source = format!("flowchart TD\nA[\"{}\"]\n", "#".repeat(1_025));
        let graph = parse_mermaid(&literal_hash_source).unwrap();
        let canonical = format_mermaid(&graph).unwrap();
        assert_eq!(parse_mermaid(&canonical).unwrap(), graph);

        let cannot_canonicalize = format!("flowchart TD\nA[\"{}\"]\n", "#".repeat(2_048));
        assert_eq!(
            parse_mermaid(&cannot_canonicalize).unwrap_err().kind,
            MermaidDiagnosticKind::LimitExceeded
        );

        let line_limited_graph = DiagramDocument {
            nodes: vec![DiagramNode {
                id: NodeId::new("A").unwrap(),
                label: "#".repeat(2_048),
                shape: NodeShape::Rectangle,
                position: Position::default(),
                group: None,
            }],
            ..DiagramDocument::default()
        };
        assert!(matches!(
            format_mermaid(&line_limited_graph),
            Err(MermaidFormatError::LineTooLarge { .. })
        ));

        let mut total_limited_graph = DiagramDocument::default();
        for index in 0..16 {
            total_limited_graph.nodes.push(DiagramNode {
                id: NodeId::new(format!("N{index}")).unwrap(),
                label: "x".repeat(crate::diagram::MAX_LABEL_BYTES),
                shape: NodeShape::Rectangle,
                position: Position::default(),
                group: None,
            });
        }
        assert!(matches!(
            format_mermaid(&total_limited_graph),
            Err(MermaidFormatError::SourceTooLarge { .. })
        ));

        let mut plain_labels = String::from("flowchart TD\n");
        for index in 0..12 {
            plain_labels.push_str(&format!("N{index}[\"{}\"]\n", "x".repeat(1_000)));
        }
        let plain_graph = parse_mermaid(&plain_labels).unwrap();
        let plain_canonical = format_mermaid(&plain_graph).unwrap();
        assert!(plain_canonical.len() < MAX_MERMAID_SOURCE_BYTES);
        assert_eq!(parse_mermaid(&plain_canonical).unwrap(), plain_graph);
    }
}
