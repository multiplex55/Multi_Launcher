//! Bounded Diagram Studio model, native document state, and scene geometry.

mod codec;
mod document;
mod export;
mod layout;
mod mermaid;
mod model;
mod persistence;
mod rendering;
mod scene;
mod session;
mod templates;
mod visual;

pub use codec::{
    DIAGRAM_SESSION_FORMAT_VERSION, DIAGRAM_VISUAL_DOCUMENT_FORMAT_VERSION,
    DIAGRAM_VISUAL_LEGACY_FORMAT_VERSION, DiagramCodecError, MAX_DIAGRAM_SESSION_BYTES,
    MAX_VISUAL_DOCUMENT_TITLE_BYTES, decode_session, encode_session,
};
pub use document::{
    DiagramAuthoringKind, DiagramDocumentController, DiagramDocumentError, DiagramSourceCopy,
    DiagramSourceCopyOrigin, PendingDiagramTransition, TransitionDecision, TransitionOutcome,
};
pub use export::{
    DiagramExportArtifact, DiagramExportError, DiagramExportFormat, DiagramExportWarnings,
    DiagramImageSnapshot, DiagramRenderedImageSnapshot, DiagramSourceSnapshot, MAX_EXPORT_PIXELS,
    MAX_EXPORT_SIDE, MAX_PNG_BYTES, MAX_PREPARED_GLYPHS, MAX_RASTER_PIXEL_VISITS, MAX_SVG_BYTES,
};
pub use visual::{
    DEFAULT_LAYER_ID, MAX_VISUAL_COORDINATE_ABS, MAX_VISUAL_DIMENSION, MAX_VISUAL_HISTORY_BYTES,
    MAX_VISUAL_HISTORY_ENTRIES, MAX_VISUAL_LAYERS, MAX_VISUAL_NAME_BYTES, MAX_VISUAL_OBJECTS,
    MAX_VISUAL_PRESET_COMPONENTS, MAX_VISUAL_PRESETS, MAX_VISUAL_SCENE_BYTES,
    MAX_VISUAL_TEXT_BYTES, MAX_VISUAL_TRANSACTION_OPERATIONS, MIN_VISUAL_DIMENSION,
    TITLE_OBJECT_ID, VisualAssetId, VisualColor, VisualDiagramDocument, VisualDocumentError,
    VisualLayer, VisualLayerId, VisualLineStyle, VisualObject, VisualObjectDraft, VisualObjectId,
    VisualObjectKind, VisualOperation, VisualPreset, VisualPresetDraft, VisualPresetId,
    VisualPrimitiveComponent, VisualPrimitiveShape, VisualScene, VisualShadow, VisualStyle,
    VisualTransactionOutcome, VisualTransform,
};

pub use layout::{DiagramLayoutError, layout_positions};
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
pub use persistence::{
    DiagramExportPublicationError, DiagramPersistenceError, DiskConflictKind,
    MAX_DIAGRAM_DOCUMENT_BYTES, MermaidFile, MermaidImportError,
};
pub use rendering::{
    DiagramFamily, DiagramRenderError, DiagramRenderOutput, RasterImage, RasterSize,
    RenderDocumentIdentity, RenderKey, RenderOptions, RenderTheme, render_mermaid_preview,
    render_mermaid_preview_with_options,
};
pub(crate) use rendering::{
    GenericPreviewStatus, RendererService, generic_preview_family, is_generic_preview_candidate,
};
pub use scene::{
    ArrowHead, CubicBezier, DiagramScene, MAX_SCENE_COORDINATE_ABS, PreparedText, PreparedTextLine,
    SceneEdge, SceneError, SceneGroup, SceneMetrics, SceneNode, ScenePoint, SceneRect,
    SceneTextMetrics, TextHorizontalAlignment, build_scene,
};
pub use session::{
    DiagramSession, DiagramSessionError, GraphOperation, MAX_DIAGRAM_HISTORY_BYTES,
    MAX_DIAGRAM_HISTORY_ENTRIES, MAX_DIAGRAM_TRANSACTION_OPERATIONS, NodePosition, PreviewState,
    SavedContentSnapshot, SessionStatus,
};
pub use templates::{DiagramTemplate, DiagramTemplateCapability, DiagramTemplateError};
