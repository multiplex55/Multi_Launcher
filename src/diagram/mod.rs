//! Bounded Diagram Studio model, native document state, and scene geometry.

mod assets;
mod codec;
mod conversion;
mod document;
mod drawio;
mod export;
mod layout;
mod mermaid;
mod mermaid_assistance;
mod model;
mod persistence;
mod recents;
mod rendering;
mod scene;
mod session;
mod templates;
mod visual;
mod visual_canvas;
mod visual_scene;
pub(crate) use visual_scene::seed_manual_route;
mod visual_templates;

pub(crate) use assets::{
    DecodedVisualAsset, PreparedVisualAsset, decode_visual_asset_payload,
    prepare_visual_asset_bytes, prepare_visual_asset_file,
};

pub use codec::{
    DIAGRAM_SESSION_FORMAT_VERSION, DIAGRAM_VISUAL_DOCUMENT_FORMAT_VERSION,
    DIAGRAM_VISUAL_LEGACY_FORMAT_VERSION, DiagramCodecError, MAX_DIAGRAM_SESSION_BYTES,
    MAX_VISUAL_DOCUMENT_TITLE_BYTES, decode_session, encode_session,
};
pub(crate) use conversion::mermaid_visual_copy;
pub use document::{
    DiagramAuthoringKind, DiagramDocumentController, DiagramDocumentError, DiagramSourceCopy,
    DiagramSourceCopyOrigin, PendingDiagramTransition, TransitionDecision, TransitionOutcome,
};
pub(crate) use drawio::export_drawio_scene;
pub use drawio::{
    DrawioExportResult, DrawioImportResult, DrawioIssue, DrawioReport, DrawioWireError,
    MAX_DRAWIO_ATTRIBUTE_NAME_BYTES, MAX_DRAWIO_ATTRIBUTE_VALUE_BYTES,
    MAX_DRAWIO_ATTRIBUTES_PER_ELEMENT, MAX_DRAWIO_CELLS, MAX_DRAWIO_ENCODED_ATTRIBUTE_BYTES,
    MAX_DRAWIO_ID_BYTES, MAX_DRAWIO_ISSUES, MAX_DRAWIO_STYLE_BYTES, MAX_DRAWIO_XML_BYTES,
    MAX_DRAWIO_XML_DEPTH, export_drawio_xml, import_drawio_xml,
};
pub use export::{
    DiagramExportArtifact, DiagramExportError, DiagramExportFormat, DiagramExportWarnings,
    DiagramImageSnapshot, DiagramRenderedImageSnapshot, DiagramSourceSnapshot,
    DiagramVisualExportOptions, DiagramVisualSnapshot, MAX_EXPORT_PIXELS, MAX_EXPORT_SIDE,
    MAX_PNG_BYTES, MAX_PREPARED_GLYPHS, MAX_RASTER_PIXEL_VISITS, MAX_SVG_BYTES,
};
pub use visual::{
    DEFAULT_LAYER_ID, MAX_VISUAL_ASSET_BYTES, MAX_VISUAL_ASSET_DIMENSION, MAX_VISUAL_ASSET_PIXELS,
    MAX_VISUAL_ASSET_TOTAL_BYTES, MAX_VISUAL_ASSETS, MAX_VISUAL_CONNECTOR_LABEL_BYTES,
    MAX_VISUAL_CONTAINMENT_DEPTH, MAX_VISUAL_CONNECTOR_POINTS, MAX_VISUAL_COORDINATE_ABS, MAX_VISUAL_DIMENSION,
    MAX_VISUAL_FRAME_LABEL_BYTES, MAX_VISUAL_HISTORY_BYTES, MAX_VISUAL_HISTORY_ENTRIES,
    MAX_VISUAL_LAYERS, MAX_VISUAL_NAME_BYTES, MAX_VISUAL_OBJECTS, MAX_VISUAL_PRESET_COMPONENTS,
    MAX_VISUAL_PRESETS, MAX_VISUAL_SCENE_BYTES, MAX_VISUAL_TEXT_BYTES,
    MAX_VISUAL_TRANSACTION_OPERATIONS, MIN_VISUAL_DIMENSION, TITLE_OBJECT_ID, VisualAnchor,
    VisualAsset, VisualAssetFormat, VisualAssetId, VisualClipboardFragment, VisualColor, VisualConnector,
    VisualConnectorDirection, VisualConnectorEndpoint, VisualConnectorRoute, VisualDiagramDocument,
    VisualDocumentError, VisualLayer, VisualLayerId, VisualLineStyle, VisualObject,
    VisualObjectDraft, VisualObjectId, VisualObjectKind, VisualOperation, VisualOwnedLabel, VisualManualRoute, VisualRoutePoint, VisualPreset,
    VisualPresetDraft, VisualPresetId, VisualPrimitiveComponent, VisualPrimitiveShape, VisualScene,
    VisualShadow, VisualStyle, VisualSwimlaneOrientation, VisualTransactionOutcome,
    VisualTransform,
};
pub use visual_canvas::{
    CanvasAnchor, CanvasGuide, CanvasGuideAxis, CanvasHandle, CanvasHandleKind, CanvasHit,
    CanvasMapping, CanvasMinimap, ResolvedVisualMembership, SnappedTranslation,
    VISUAL_GUIDE_TOLERANCE_SCREEN_POINTS, VISUAL_LAYOUT_GRID_SPACING, VisualAlign,
    VisualDistribution, VisualTransformMode, align_selection as visual_canvas_align_selection,
    distribute_selection as visual_canvas_distribute_selection, hit_test as visual_canvas_hit_test,
    marquee_selection as visual_canvas_marquee_selection,
    minimap_layout as visual_canvas_minimap_layout,
    preview_transforms as preview_visual_transforms,
    property_transform_operations as visual_canvas_property_transform_operations,
    resolve_visual_membership as visual_canvas_resolve_membership,
    selection_bounds as visual_canvas_selection_bounds,
    selection_handles as visual_canvas_selection_handles,
    snap_translation_delta as visual_canvas_snap_translation_delta,
};
pub use visual_scene::{
    MAX_VISUAL_DASH_SEGMENTS_PER_ITEM, MAX_VISUAL_DASH_SEGMENTS_PER_SCENE,
    MAX_VISUAL_OUTLINE_POINTS, MAX_VISUAL_PROJECTED_ITEMS, MAX_VISUAL_PROJECTION_COORDINATE_ABS,
    MAX_VISUAL_TEXT_LINES_PER_OBJECT, MAX_VISUAL_TEXT_LINES_PER_SCENE, ProjectedStrokePrimitive,
    ProjectedVisualItem, VisualProjectedKind, VisualProjectionError, VisualSceneProjection,
    color_with_opacity, project_visual_document, project_visual_scene,
};

pub use layout::{DiagramLayoutError, layout_positions};
pub use mermaid::{
    MAX_MERMAID_LINE_BYTES, MAX_MERMAID_SOURCE_BYTES, MAX_MERMAID_SOURCE_LINES, MermaidDiagnostic,
    MermaidDiagnosticKind, MermaidFormatError, SourceSpan, format_mermaid, parse_mermaid,
};
pub use mermaid_assistance::{
    MermaidAssistance, MermaidAssistanceContext, MermaidAssistanceSuggestion,
    apply_mermaid_assistance, mermaid_assistance_at,
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
pub(crate) use persistence::{
    DrawioFileError, duplicate_saved_document, publish_drawio_export, publish_visual_document_copy,
    read_drawio_xml_source,
};
pub(crate) use recents::RecentDiagramIndex;
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

pub use visual_templates::VisualAuthoringTemplate;

pub(crate) use persistence::{
    ResolvedExportDestination, StagedUnoccupiedFile, resolve_export_destination,
    stage_export_unoccupied,
};
