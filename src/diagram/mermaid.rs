//! Safe parser and canonical formatter for the documented Mermaid subset.

use super::model::{
    DiagramDocument, DiagramEdge, DiagramGroup, DiagramNode, EdgeId, FlowDirection,
    GraphValidationError, GroupId, MAX_DIAGRAM_EDGES, MAX_DIAGRAM_GROUPS, MAX_DIAGRAM_NODES,
    MAX_GROUP_ID_BYTES, MAX_LABEL_BYTES, MAX_NODE_ID_BYTES, NodeId, NodeShape, Position,
};
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
    SourceTooLarge { max_bytes: usize },
    LineTooLarge { max_bytes: usize },
    TooManyLines { max_lines: usize },
}

impl fmt::Display for MermaidFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGraph(error) => error.fmt(formatter),
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
            Self::SourceTooLarge { .. } | Self::LineTooLarge { .. } | Self::TooManyLines { .. } => {
                None
            }
        }
    }
}

/// Parse the entire source or return one precise invalid/unsupported diagnostic.
///
/// A blank draft is a valid empty top-down graph. Nonblank source must begin with
/// a `flowchart TD`, `flowchart TB`, or `flowchart LR` header.
pub fn parse_mermaid(source: &str) -> Result<DiagramDocument, MermaidDiagnostic> {
    preflight_source(source)?;
    if source.trim().is_empty() {
        return Ok(DiagramDocument::default());
    }

    let mut graph = DiagramDocument::default();
    let mut nodes_by_id = HashMap::<NodeId, usize>::new();
    let mut explicit_declarations = HashMap::<NodeId, (NodeShape, String)>::new();
    let mut active_group: Option<GroupId> = None;
    let mut active_group_span = None;
    let mut saw_header = false;
    let mut byte_offset = 0usize;
    let mut line_number = 1usize;

    for line_with_newline in source.split_inclusive('\n') {
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
                line_number,
                start,
                line.len(),
            ));
        }

        let trimmed = line.trim();
        if !trimmed.is_empty() {
            if !saw_header {
                graph.direction = parse_header(line, byte_offset, line_number)?;
                saw_header = true;
            } else {
                let leading = line.len() - line.trim_start().len();
                if trimmed.starts_with("%%") {
                    return Err(diagnostic(
                        MermaidDiagnosticKind::Unsupported,
                        "comments and directives are preserved as unsupported source",
                        line,
                        byte_offset,
                        line_number,
                        leading,
                        line.len(),
                    ));
                }
                if trimmed == "end" {
                    if active_group.take().is_none() {
                        return Err(diagnostic(
                            MermaidDiagnosticKind::Invalid,
                            "`end` does not have an open subgraph",
                            line,
                            byte_offset,
                            line_number,
                            leading,
                            line.len(),
                        ));
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
                    active_group = Some(group);
                    active_group_span = Some((line.to_owned(), byte_offset, line_number, leading));
                } else {
                    parse_statement(
                        line,
                        byte_offset,
                        line_number,
                        active_group.as_ref(),
                        &mut graph,
                        &mut nodes_by_id,
                        &mut explicit_declarations,
                    )?;
                }
            }
        }
        byte_offset = byte_offset.saturating_add(line_with_newline.len());
        line_number += 1;
    }

    if !saw_header {
        return Err(diagnostic(
            MermaidDiagnosticKind::Invalid,
            "expected a `flowchart TD`, `flowchart TB`, or `flowchart LR` header",
            source,
            0,
            1,
            0,
            source.chars().next().map_or(0, char::len_utf8),
        ));
    }
    if let Some((line, offset, number, column)) = active_group_span {
        return Err(diagnostic(
            MermaidDiagnosticKind::Invalid,
            "subgraph is missing its closing `end`",
            &line,
            offset,
            number,
            column,
            line.len(),
        ));
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
    Ok(graph)
}

/// Emit normalized supported Mermaid syntax. The input graph is never mutated.
pub fn format_mermaid(graph: &DiagramDocument) -> Result<String, MermaidFormatError> {
    graph.validate().map_err(MermaidFormatError::InvalidGraph)?;
    let encoded_bytes = measure_formatted_size(graph)?;

    let mut output = String::with_capacity(encoded_bytes);
    output.push_str("flowchart ");
    output.push_str(graph.direction.as_mermaid());
    output.push('\n');

    let mut emitted_groups = Vec::<GroupId>::with_capacity(graph.groups.len());
    for node in &graph.nodes {
        match &node.group {
            None => write_node_line(&mut output, node),
            Some(group_id) if !emitted_groups.contains(group_id) => {
                if let Some(group) = graph.groups.iter().find(|group| &group.id == group_id) {
                    emitted_groups.push(group_id.clone());
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
                        write_node_line(&mut output, member);
                    }
                    output.push_str("end\n");
                }
            }
            Some(_) => {}
        }
    }
    for group in &graph.groups {
        if !emitted_groups.contains(&group.id) {
            output.push_str("subgraph ");
            output.push_str(group.id.as_str());
            output.push_str("[\"");
            encode_mermaid_text(&group.label, &mut output);
            output.push_str("\"]\nend\n");
        }
    }
    for edge in &graph.edges {
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
    Ok(output)
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
    record_formatted_line(&mut total_bytes, &mut line_count, "flowchart TD".len())?;

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

fn preflight_source(source: &str) -> Result<(), MermaidDiagnostic> {
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
    Ok(())
}

fn parse_header(
    line: &str,
    byte_offset: usize,
    line_number: usize,
) -> Result<FlowDirection, MermaidDiagnostic> {
    let leading = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    let Some(after_keyword) = trimmed.strip_prefix("flowchart") else {
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
        "BT" | "RL" => Err(diagnostic(
            MermaidDiagnosticKind::Unsupported,
            format!(
                "flowchart direction {} is outside the TD/TB/LR subset",
                after_keyword.trim()
            ),
            line,
            byte_offset,
            line_number,
            leading,
            line.len(),
        )),
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

fn is_subgraph_keyword(trimmed: &str) -> bool {
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
) -> Result<(), MermaidDiagnostic> {
    let mut parser = StatementParser::new(line, byte_offset, line_number);
    parser.skip_whitespace();
    let first = parser.parse_node_term()?;
    parser.skip_whitespace();
    if parser.remaining().starts_with("-->") {
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
        if !parser.is_finished() {
            return Err(parser.error(
                MermaidDiagnosticKind::Unsupported,
                "chained or trailing statements are outside the supported one-statement-per-line subset",
                parser.position,
                line.len(),
            ));
        }
        let from = ensure_node(
            graph,
            nodes_by_id,
            explicit_declarations,
            first,
            active_group,
            &parser,
        )?;
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
                0,
                line.len(),
            ));
        }
        let id = graph
            .allocate_edge_id()
            .map_err(|error| parser.graph_error(error, 0))?;
        graph.edges.push(DiagramEdge {
            id,
            from,
            to,
            label: edge_label,
        });
    } else {
        if !parser.is_finished() {
            return Err(parser.error(
                MermaidDiagnosticKind::Unsupported,
                "unsupported token or trailing syntax in flowchart statement",
                parser.position,
                line.len(),
            ));
        }
        let _ = ensure_node(
            graph,
            nodes_by_id,
            explicit_declarations,
            first,
            active_group,
            &parser,
        )?;
    }
    Ok(())
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
            "flowchart TD\n%% retained comment\nA --> B\n",
            "flowchart TD\n%%{init: {\"theme\":\"dark\"}}%%\n",
            "flowchart TD\nA --> B --> C\n",
            "flowchart TD\nA --> B; C --> D\n",
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
