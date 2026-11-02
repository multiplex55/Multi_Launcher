//! Pure graph model, Mermaid synchronization, and bounded session codec.

mod codec;
mod mermaid;
mod model;
mod session;

pub use codec::{
    DIAGRAM_SESSION_FORMAT_VERSION, DiagramCodecError, MAX_DIAGRAM_SESSION_BYTES, decode_session,
    encode_session,
};

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
pub use session::{
    DiagramSession, DiagramSessionError, GraphOperation, MAX_DIAGRAM_HISTORY_BYTES,
    MAX_DIAGRAM_HISTORY_ENTRIES, MAX_DIAGRAM_TRANSACTION_OPERATIONS, NodePosition, PreviewState,
    SavedContentSnapshot, SessionStatus,
};
