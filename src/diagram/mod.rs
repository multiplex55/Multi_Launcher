//! Pure canonical graph and bounded Mermaid flowchart subset.

mod mermaid;
mod model;

pub use mermaid::{
    MAX_MERMAID_LINE_BYTES, MAX_MERMAID_SOURCE_BYTES, MAX_MERMAID_SOURCE_LINES, MermaidDiagnostic,
    MermaidDiagnosticKind, MermaidFormatError, SourceSpan, format_mermaid, parse_mermaid,
};
pub use model::{
    DiagramDocument, DiagramEdge, DiagramGroup, DiagramNode, EdgeId, FlowDirection,
    GraphValidationError, GroupId, MAX_DIAGRAM_EDGES, MAX_DIAGRAM_GROUPS, MAX_DIAGRAM_NODES,
    MAX_GROUP_ID_BYTES, MAX_LABEL_BYTES, MAX_NODE_ID_BYTES, MAX_POSITION_ABS, NodeId, NodeShape,
    Position,
};
