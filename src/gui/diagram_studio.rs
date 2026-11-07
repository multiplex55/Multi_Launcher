#[cfg(windows)]
use super::native_keyboard::reserve_windows_system_menu_alt_space;
use super::utility_dialog_size::{UtilityDialogSizeWindowExt, UtilityWindowSizeRecipe};
use super::utility_host::{
    HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
    presentation_button,
};
use super::utility_window_geometry::NativeViewportIdentity;
use crate::diagram::{
    CanvasGuide, CanvasGuideAxis, CanvasHandle, CanvasHandleKind, CanvasHit, CanvasMapping,
    CanvasMinimap, DEFAULT_LAYER_ID, DecodedVisualAsset, DiagramDocument,
    DiagramDocumentController, DiagramDocumentError, DiagramExportFormat, DiagramImageSnapshot,
    DiagramRenderOutput, DiagramRenderedImageSnapshot, DiagramScene, DiagramSession,
    DiagramSourceCopy, DiagramSourceCopyOrigin, DiagramSourceSnapshot, DiagramTemplate,
    DrawioExportResult, DrawioImportResult, DrawioReport, DrawioWireError, EdgeId, FlowDirection,
    GenericPreviewStatus, GraphOperation, GroupId, MAX_DIAGRAM_GROUPS, MAX_DIAGRAM_NODES,
    MAX_VISUAL_CONNECTOR_LABEL_BYTES, MAX_VISUAL_PRESET_COMPONENTS,
    MAX_VISUAL_TRANSACTION_OPERATIONS, MermaidFile, NodeId, NodePosition, NodeShape,
    PendingDiagramTransition, Position, PreparedText, PreparedVisualAsset, PreviewState,
    ProjectedStrokePrimitive, RasterSize, RecentDiagramIndex, RenderDocumentIdentity, RenderKey,
    RenderOptions, RendererService, SceneMetrics, ScenePoint, SceneRect, SessionStatus,
    TextHorizontalAlignment, TransitionDecision, TransitionOutcome, VisualAlign, VisualAnchor,
    VisualAsset, VisualAssetId, VisualColor, VisualConnector, VisualConnectorDirection,
    VisualConnectorEndpoint, VisualConnectorRoute, VisualDistribution, VisualLineStyle,
    VisualObject, VisualObjectDraft, VisualObjectId, VisualObjectKind, VisualOperation,
    VisualPresetDraft, VisualPrimitiveShape, VisualProjectedKind, VisualProjectionError,
    VisualScene, VisualSceneProjection, VisualShadow, VisualStyle, VisualSwimlaneOrientation,
    VisualTransform, VisualTransformMode, build_scene, color_with_opacity,
    duplicate_saved_document, export_drawio_scene, generic_preview_family, import_drawio_xml,
    is_generic_preview_candidate, mermaid_visual_copy, preview_visual_transforms,
    project_visual_scene, publish_drawio_export, publish_visual_document_copy,
    read_drawio_xml_source, visual_canvas_align_selection, visual_canvas_distribute_selection,
    visual_canvas_hit_test, visual_canvas_marquee_selection, visual_canvas_minimap_layout,
    visual_canvas_selection_bounds, visual_canvas_selection_handles,
    visual_canvas_snap_translation_delta,
};
use crate::diagram::{
    DiagramExportWarnings, DiagramVisualExportOptions, DiagramVisualSnapshot,
    ResolvedExportDestination, StagedUnoccupiedFile, resolve_export_destination,
    stage_export_unoccupied,
};
use eframe::egui;
use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};

const DEFAULT_VIEWPORT_SIZE: [f32; 2] = [1180.0, 760.0];
const MIN_VIEWPORT_SIZE: [f32; 2] = [480.0, 360.0];
const VIEWER_VIEWPORT_SIZE: [f32; 2] = [920.0, 700.0];
const MIN_VIEWER_VIEWPORT_SIZE: [f32; 2] = [400.0, 320.0];
const CAMERA_MIN_ZOOM: f32 = 0.025;
const CAMERA_MAX_ZOOM: f32 = 8.0;
const CAMERA_KEYBOARD_PAN_SCREEN_POINTS: f64 = 64.0;
const CAMERA_KEYBOARD_ZOOM_FACTOR: f32 = 1.2;
const GRID_SPACING: f32 = 10.0;
const DRAG_NODE_SEPARATION: f64 = 120.0;
const SPLITTER_WIDTH: f32 = 8.0;
const SOURCE_PANE_MIN_WIDTH: f32 = 280.0;
const PREVIEW_PANE_MIN_WIDTH: f32 = 340.0;
const INSPECTOR_PANE_MIN_WIDTH: f32 = 180.0;
const CANVAS_MIN_WIDTH: f32 = 220.0;
const CANVAS_MIN_HEIGHT: f32 = 300.0;
const DEFAULT_SOURCE_PREVIEW_RATIO: f32 = 0.42;
const DEFAULT_INSPECTOR_CANVAS_RATIO: f32 = 0.38;
const VISUAL_TWO_COLUMN_MIN_WIDTH: f32 = 460.0;
const SPLIT_TWO_COLUMN_MIN_WIDTH: f32 = 860.0;
const NATIVE_DIAGRAM_EXTENSION: &str = "mld";

pub(super) fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("diagram_studio_viewport")
}

pub(super) fn viewer_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("diagram_studio_preview_viewer")
}

fn viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Diagram Studio")
        .with_inner_size(DEFAULT_VIEWPORT_SIZE)
        .with_min_inner_size(MIN_VIEWPORT_SIZE)
        .with_resizable(true)
        .with_visible(true)
}

fn viewer_viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Diagram Viewer")
        .with_inner_size(VIEWER_VIEWPORT_SIZE)
        .with_min_inner_size(MIN_VIEWER_VIEWPORT_SIZE)
        .with_resizable(true)
        .with_visible(true)
}

fn native_geometry_identity(
    key: &'static str,
    viewport_id: egui::ViewportId,
    owner_generation: u64,
    host: HostIdentity,
) -> Option<NativeViewportIdentity> {
    (host.host == UtilityPresentation::Popout).then_some(NativeViewportIdentity {
        key: crate::settings::UtilityToolKey::parse(key).ok()?,
        viewport_id,
        owner_generation,
        host_generation: host.generation,
    })
}

struct DiagramHost {
    title: &'static str,
    key: &'static str,
    viewport: egui::ViewportId,
    builder: egui::ViewportBuilder,
    size: [f32; 2],
    minimum: [f32; 2],
}

impl DiagramHost {
    fn show(
        self,
        ctx: &egui::Context,
        presentation: UtilityPresentation,
        root_visible: bool,
        root_input_allowed: bool,
        mut body: impl FnMut(&mut egui::Ui, egui::ViewportClass),
    ) -> bool {
        if presentation == UtilityPresentation::Popout {
            ctx.show_viewport_immediate(self.viewport, self.builder, body);
            return false;
        }
        if !root_visible {
            return false;
        }
        let available = ctx.content_rect().size();
        let minimum = egui::Vec2::from(self.minimum).min(available);
        let default_size = egui::Vec2::from(self.size)
            .min(available * 0.85)
            .max(minimum);
        let id = diagram_host_layer(self.key).id;
        let mut open = true;
        egui::Window::new(self.title)
            .id(id)
            .enabled(root_input_allowed)
            .collapsible(false)
            .resizable(true)
            .min_size(minimum)
            .max_size(available)
            .show_utility(
                ctx,
                self.key,
                id,
                Some(&mut open),
                UtilityWindowSizeRecipe::outer(default_size)
                    .with_minimum(minimum)
                    .with_maximum(available),
                |ui| body(ui, egui::ViewportClass::Root),
            );
        !open
    }
}

fn diagram_host_layer(key: &str) -> egui::LayerId {
    egui::LayerId::new(egui::Order::Middle, egui::Id::new(key))
}

fn transition_diagram_host(
    ctx: &egui::Context,
    host: &mut UtilityHostState,
    viewport: egui::ViewportId,
) -> bool {
    let docked = host.presentation() == UtilityPresentation::Popout;
    host.transition(if docked {
        UtilityPresentation::Docked
    } else {
        UtilityPresentation::Popout
    });
    if docked {
        ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Close);
    }
    ctx.request_repaint_of(egui::ViewportId::ROOT);
    docked
}

fn active_pointer_position(ctx: &egui::Context) -> Option<egui::Pos2> {
    ctx.input(|input| {
        (input.pointer.any_pressed() || input.pointer.any_released())
            .then(|| input.pointer.interact_pos())
            .flatten()
    })
}

fn focused_on_layer(ctx: &egui::Context, layer: egui::LayerId) -> Option<egui::Id> {
    ctx.memory(|memory| memory.focused()).filter(|id| {
        ctx.read_response(*id)
            .is_some_and(|response| response.layer_id == layer)
    })
}

fn ensure_native_extension(mut path: PathBuf) -> PathBuf {
    let has_native_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(NATIVE_DIAGRAM_EXTENSION));
    if !has_native_extension {
        path.set_extension(NATIVE_DIAGRAM_EXTENSION);
    }
    path
}

fn ensure_drawio_extension(mut path: PathBuf) -> PathBuf {
    let has_drawio_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("drawio"));
    if !has_drawio_extension {
        path.set_extension("drawio");
    }
    path
}

fn ensure_export_extension(mut path: PathBuf, format: DiagramExportFormat) -> PathBuf {
    let has_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| !extension.is_empty());
    if !has_extension {
        path.set_extension(format.extension());
    }
    path
}

const fn export_format_name(format: DiagramExportFormat) -> &'static str {
    match format {
        DiagramExportFormat::Mermaid => "Mermaid source",
        DiagramExportFormat::Svg => "SVG image",
        DiagramExportFormat::Png => "PNG image",
    }
}

const fn export_filter_name(format: DiagramExportFormat) -> &'static str {
    match format {
        DiagramExportFormat::Mermaid => "Mermaid source (*.mmd)",
        DiagramExportFormat::Svg => "SVG image (*.svg)",
        DiagramExportFormat::Png => "PNG image (*.png)",
    }
}

fn same_export_destination(native_path: &Path, export_path: &Path) -> io::Result<bool> {
    paths_equal_for_export(
        &resolved_export_path_identity(native_path)?,
        &resolved_export_path_identity(export_path)?,
    )
}

fn resolved_export_path_identity(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let absolute = lexically_normalize(&strip_windows_verbatim_prefix(&absolute));
    let mut existing = absolute.clone();
    let mut missing_components: Vec<OsString> = Vec::new();
    loop {
        match fs::symlink_metadata(&existing) {
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(file_name) = existing.file_name() else {
                    return Err(error);
                };
                missing_components.push(file_name.to_os_string());
                let Some(parent) = existing.parent() else {
                    return Err(error);
                };
                existing = parent.to_path_buf();
            }
            Err(error) => return Err(error),
        }
    }

    let mut resolved = fs::canonicalize(existing)?;
    for component in missing_components.iter().rev() {
        resolved.push(component);
    }
    Ok(lexically_normalize(&strip_windows_verbatim_prefix(
        &resolved,
    )))
}

#[cfg(windows)]
fn paths_equal_for_export(left: &Path, right: &Path) -> io::Result<bool> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};

    let left = left.as_os_str().encode_wide().collect::<Vec<_>>();
    let right = right.as_os_str().encode_wide().collect::<Vec<_>>();
    if left.len() > i32::MAX as usize || right.len() > i32::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "export path exceeds the Windows comparison length limit",
        ));
    }
    if left.is_empty() || right.is_empty() {
        return Ok(left.is_empty() && right.is_empty());
    }
    // SAFETY: both UTF-16 buffers stay alive for the call, their lengths fit
    // the Windows API's i32 parameters, and CompareStringOrdinal only compares
    // the supplied slices without retaining pointers or opening either path.
    let result = unsafe { CompareStringOrdinal(&left, &right, BOOL(1)) };
    if result.0 == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(result == CSTR_EQUAL)
}

#[cfg(not(windows))]
fn paths_equal_for_export(left: &Path, right: &Path) -> io::Result<bool> {
    Ok(left == right)
}

fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.file_name().is_some() {
                    normalized.pop();
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        const VERBATIM_UNC_PREFIX: [u16; 8] = [92, 92, 63, 92, 85, 78, 67, 92];
        const VERBATIM_PREFIX: [u16; 4] = [92, 92, 63, 92];
        let wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
        let is_verbatim_unc = wide.len() >= VERBATIM_UNC_PREFIX.len()
            && wide[..4] == VERBATIM_PREFIX
            && matches!(wide[4], 85 | 117)
            && matches!(wide[5], 78 | 110)
            && matches!(wide[6], 67 | 99)
            && wide[7] == VERBATIM_UNC_PREFIX[7];
        if is_verbatim_unc {
            let mut normalized = vec![92, 92];
            normalized.extend_from_slice(&wide[VERBATIM_UNC_PREFIX.len()..]);
            return PathBuf::from(OsString::from_wide(&normalized));
        }
        if wide.starts_with(&VERBATIM_PREFIX) {
            return PathBuf::from(OsString::from_wide(&wide[VERBATIM_PREFIX.len()..]));
        }
    }
    path.to_path_buf()
}

#[cfg(feature = "qa-framebuffer-capture")]
#[derive(Clone)]
pub(super) struct DiagramViewportObservation {
    pub child_viewport_id: egui::ViewportId,
    pub callback_class: egui::ViewportClass,
    pub callback_frame_nr: u64,
    pub callback_pass_nr: u64,
    pub callback_pass_index: usize,
    pub visible: Option<bool>,
    pub minimized: Option<bool>,
    pub occluded: Option<bool>,
    pub focused: Option<bool>,
    pub body_rendered: bool,
    pub still_open: bool,
    pub application_input_allowed: bool,
    pub child_modal_open: bool,
    pub pixels_per_point: f32,
    pub native_pixels_per_point: Option<f32>,
    pub zoom_factor: f32,
    pub session_revision: u64,
    pub rendered_scene_revision: Option<u64>,
    pub status: SessionStatus,
    pub callback_observed_at_unix_ms: u64,
    pub completed_frame_nr: Option<u64>,
    pub completed_pass_nr: Option<u64>,
    pub renderer_returned_at_unix_ms: Option<u64>,
}

#[cfg(feature = "qa-framebuffer-capture")]
impl DiagramViewportObservation {
    fn is_immediate_target(&self) -> bool {
        self.callback_class == egui::ViewportClass::Immediate
            && self.child_viewport_id == viewport_id()
            && self.body_rendered
            && self.still_open
    }
}

#[cfg(feature = "qa-framebuffer-capture")]
fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ViewMode {
    Source {
        restore: PreviewMode,
    },
    Visual,
    #[default]
    Split,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewMode {
    Visual,
    Split,
}

impl ViewMode {
    fn hide_preview(self) -> Self {
        match self {
            Self::Source { .. } => self,
            Self::Visual => Self::Source {
                restore: PreviewMode::Visual,
            },
            Self::Split => Self::Source {
                restore: PreviewMode::Split,
            },
        }
    }

    fn shows_preview(self) -> Self {
        match self {
            Self::Source {
                restore: PreviewMode::Visual,
            } => Self::Visual,
            Self::Source {
                restore: PreviewMode::Split,
            } => Self::Split,
            mode => mode,
        }
    }

    const fn from_preview_mode(mode: PreviewMode) -> Self {
        match mode {
            PreviewMode::Visual => Self::Visual,
            PreviewMode::Split => Self::Split,
        }
    }

    const fn is_source(self) -> bool {
        matches!(self, Self::Source { .. })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ExportScale {
    Half,
    #[default]
    One,
    OneAndHalf,
    Two,
}

impl ExportScale {
    const ALL: [(Self, &'static str, &'static str); 4] = [
        (Self::Half, "0.5×", "half"),
        (Self::One, "1×", "one"),
        (Self::OneAndHalf, "1.5×", "one_and_half"),
        (Self::Two, "2×", "two"),
    ];

    const fn factor(self) -> f64 {
        match self {
            Self::Half => 0.5,
            Self::One => 1.0,
            Self::OneAndHalf => 1.5,
            Self::Two => 2.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum StudioNotice {
    Info(String),
    Error(String),
}

impl StudioNotice {
    fn info(message: impl Into<String>) -> Self {
        Self::Info(message.into())
    }

    fn error(message: impl Into<String>) -> Self {
        Self::Error(message.into())
    }

    fn text(&self) -> &str {
        match self {
            Self::Info(message) | Self::Error(message) => message,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DiagramElement {
    Node(NodeId),
    Edge(EdgeId),
    Group(GroupId),
}

#[derive(Clone, Debug)]
enum PropertyFields {
    Node {
        label: String,
        shape: NodeShape,
        group: Option<GroupId>,
    },
    Edge {
        label: Option<String>,
        from: NodeId,
        to: NodeId,
    },
    Group {
        label: String,
    },
}

#[derive(Clone, Debug)]
struct PropertyDraft {
    target: DiagramElement,
    basis_revision: u64,
    fields: PropertyFields,
}

#[derive(Clone, Copy, Debug)]
struct Camera {
    center: ScenePoint,
    zoom: f32,
    fit_next: bool,
}

#[derive(Clone)]
struct MinimapDrag {
    layout: CanvasMinimap,
    offset: ScenePoint,
    host: HostIdentity,
    generation: u64,
    start_pass: ViewportPassProvenance,
    accepted: bool,
    released: bool,
    last_camera: Camera,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CanvasInteraction {
    Editor,
    Viewer,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: ScenePoint::default(),
            zoom: 1.0,
            fit_next: true,
        }
    }
}

#[derive(Clone, Debug)]
struct DragState {
    active_node: NodeId,
    pointer_origin: egui::Pos2,
    initial_positions: Vec<NodePosition>,
}

struct RevisionCache {
    revision: u64,
    status: SessionStatus,
    scene: Option<DiagramScene>,
    scene_error: Option<String>,
}

struct GenericPreviewTexture {
    key: RenderKey,
    texture: egui::TextureHandle,
    natural_width: f32,
    natural_height: f32,
    content_uv: egui::Rect,
}

#[derive(Clone)]
struct VisualProjectionCache {
    document: RenderDocumentIdentity,
    revision: u64,
    scene: Arc<VisualScene>,
    projection: Result<Arc<VisualSceneProjection>, VisualProjectionError>,
}

#[derive(Clone)]
struct VisualAssetTexture {
    texture: egui::TextureHandle,
    width: u32,
    height: u32,
}

#[derive(Clone)]
struct VisualAssetImportFence {
    document: RenderDocumentIdentity,
    revision: u64,
    owner_generation: u64,
    owner_viewport_id: egui::ViewportId,
    start_pass: ViewportPassProvenance,
    start_pass_accepted: bool,
}

enum VisualAssetWorkKind {
    Import(VisualAssetImportFence),
    Hydrate,
    Export,
}

#[derive(Clone, Copy)]
enum VisualAssetWorkKindTag {
    Import,
    Hydrate,
    Export,
}

enum VisualAssetWorkResult {
    Import(Result<PreparedVisualAsset, String>),
    Hydrate(Vec<Result<DecodedVisualAsset, (VisualAssetId, String)>>),
    Export(Result<StagedVisualExport, String>),
}

struct PendingVisualAssetWork {
    document: RenderDocumentIdentity,
    revision: u64,
    owner_generation: u64,
    owner_viewport_id: egui::ViewportId,
    kind: VisualAssetWorkKind,
    cancelled: Arc<AtomicBool>,
    receiver: Receiver<VisualAssetWorkResult>,
    hydration_ids: Vec<VisualAssetId>,
    retired: bool,
}

struct CompletedVisualAssetImport {
    fence: VisualAssetImportFence,
    prepared: PreparedVisualAsset,
}

struct StagedVisualExport {
    fence: DocumentActionFence,
    format: DiagramExportFormat,
    revision: u64,
    dimensions: Option<(u32, u32)>,
    warnings: crate::diagram::DiagramExportWarnings,
    staged: StagedUnoccupiedFile,
}

#[derive(Clone)]
struct DrawioWorkflowScope {
    document: RenderDocumentIdentity,
    revision: u64,
    owner_generation: u64,
    owner_viewport_id: egui::ViewportId,
}

impl DrawioWorkflowScope {
    fn matches(&self, dialog: &DiagramStudioDialog, owner_viewport_id: egui::ViewportId) -> bool {
        let revision = dialog
            .documents
            .session()
            .map(DiagramSession::revision)
            .or_else(|| dialog.documents.visual_revision())
            .unwrap_or_default();
        self.document == dialog.render_document
            && self.revision == revision
            && self.owner_generation == dialog.owner_generation
            && self.owner_viewport_id == owner_viewport_id
            && dialog.documents.pending_transition().is_none()
    }
}

enum DrawioWorkKind {
    Import { source_path: PathBuf },
    Export,
}

enum DrawioWorkResult {
    Import(Result<DrawioImportResult, DrawioImportError>),
    Export(Result<DrawioExportResult, DrawioWireError>),
}

enum DrawioImportError {
    Read(String),
    Wire(DrawioWireError),
}

struct PendingDrawioWork {
    scope: DrawioWorkflowScope,
    kind: DrawioWorkKind,
    cancelled: Arc<AtomicBool>,
    receiver: Receiver<DrawioWorkResult>,
    retired: bool,
}

enum DrawioWorkflowReview {
    Import {
        scope: DrawioWorkflowScope,
        source_path: PathBuf,
        candidate: DrawioImportResult,
        copy_path: Option<PathBuf>,
    },
    ImportUnsupported {
        scope: DrawioWorkflowScope,
        source_path: PathBuf,
        report: DrawioReport,
    },
    ExportReady {
        scope: DrawioWorkflowScope,
        result: DrawioExportResult,
    },
    ExportUnsupported {
        scope: DrawioWorkflowScope,
        report: DrawioReport,
        reason: String,
    },
}

impl DrawioWorkflowReview {
    fn scope(&self) -> &DrawioWorkflowScope {
        match self {
            Self::Import { scope, .. }
            | Self::ImportUnsupported { scope, .. }
            | Self::ExportReady { scope, .. }
            | Self::ExportUnsupported { scope, .. } => scope,
        }
    }
}

#[derive(Clone, Debug)]
struct VisualPropertyDraft {
    document: RenderDocumentIdentity,
    basis_revision: u64,
    object: VisualObject,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct VisualSelectionState {
    objects: BTreeSet<VisualObjectId>,
    primary: Option<VisualObjectId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum VisualGestureMode {
    Transform(VisualTransformMode),
    Marquee { extend: bool },
}

#[derive(Clone)]
struct VisualGesture {
    document: RenderDocumentIdentity,
    revision: u64,
    owner_generation: u64,
    owner_viewport_id: egui::ViewportId,
    start_pass: ViewportPassProvenance,
    start_pass_accepted: bool,
    mode: VisualGestureMode,
    mapping: CanvasMapping,
    pointer_start: egui::Pos2,
    pointer_start_world: ScenePoint,
    initial_selection: VisualSelectionState,
    initial_objects: Vec<VisualObject>,
    last_pointer: egui::Pos2,
    preview_objects: Vec<VisualObject>,
    preview_scene: Option<Arc<VisualScene>>,
    preview_projection: Option<Arc<VisualSceneProjection>>,
    preview_guides: Vec<CanvasGuide>,
    did_move: bool,
}

enum PendingMutation {
    ReplaceSource(String),
    ReplaceSourceWithCursor {
        source: String,
        expected_document: RenderDocumentIdentity,
        expected_revision: u64,
        editor_id: egui::Id,
        cursor_character: usize,
    },
    ReplaceVisualLabel {
        label: String,
        expected_document: RenderDocumentIdentity,
        expected_revision: u64,
    },
    VisualTransaction {
        operations: Vec<VisualOperation>,
        expected_document: RenderDocumentIdentity,
        expected_revision: u64,
        select_after: Option<VisualObjectId>,
    },
    VisualAssetImport {
        prepared: PreparedVisualAsset,
        layer_id: crate::diagram::VisualLayerId,
        transform: VisualTransform,
        style: VisualStyle,
        fence: VisualAssetImportFence,
    },
    VisualLayoutFailure {
        message: String,
        expected_document: RenderDocumentIdentity,
        expected_revision: u64,
    },
    VisualGestureTransaction {
        operations: Vec<VisualOperation>,
        expected_document: RenderDocumentIdentity,
        expected_revision: u64,
        owner_generation: u64,
        owner_viewport_id: egui::ViewportId,
        start_pass: ViewportPassProvenance,
        start_pass_accepted: bool,
    },
    VisualHistory {
        redo: bool,
        expected_document: RenderDocumentIdentity,
        expected_revision: u64,
    },
    Transaction {
        operations: Vec<GraphOperation>,
        expected_revision: u64,
        select_after: Option<DiagramElement>,
    },
    Undo,
    Redo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DocumentIntent {
    New,
    NewVisual,
    Open,
    ImportMermaid,
    ApplyTemplate(DiagramTemplate),
    ApplyVisualTemplate(crate::diagram::VisualAuthoringTemplate),
    AutoLayout,
    Save,
    SaveAs,
    ConvertMermaidCopy,
    OpenRecent(PathBuf),
    DuplicateRecent(PathBuf),
    Export(DiagramExportFormat, ExportScale),
    DrawioImport,
    DrawioExport,
    DrawioPublishImport,
    DrawioOpenImportCopy,
    DrawioPublishExport,
    DrawioCancel,
    UseSourceInMarkdown(DiagramSourceCopyOrigin),
    Close,
    ResolvePending(TransitionDecision),
}

impl DocumentIntent {
    const fn is_drawio(&self) -> bool {
        matches!(
            self,
            Self::DrawioImport
                | Self::DrawioExport
                | Self::DrawioPublishImport
                | Self::DrawioOpenImportCopy
                | Self::DrawioPublishExport
                | Self::DrawioCancel
        )
    }

    fn requires_action_fence(&self) -> bool {
        matches!(
            self,
            Self::Export(_, _) |
            Self::ApplyVisualTemplate(_)
                | Self::ConvertMermaidCopy
                | Self::OpenRecent(_)
                | Self::DuplicateRecent(_)
                | Self::DrawioImport
                | Self::DrawioExport
                | Self::DrawioPublishImport
                | Self::DrawioOpenImportCopy
                | Self::DrawioPublishExport
                | Self::DrawioCancel
        )
    }
}

#[derive(Clone, Debug)]
struct MenuFocusScope {
    popup_id: egui::Id,
    trigger_id: egui::Id,
    item_ids: Vec<egui::Id>,
    sizing_pass: bool,
}

#[derive(Clone, Copy, Debug)]
enum MenuFocusDestination {
    Item(egui::Id),
    Trigger(egui::Id),
}

#[derive(Clone, Copy, Debug)]
struct MenuFocusRequest {
    popup_id: egui::Id,
    destination: MenuFocusDestination,
}

#[derive(Default)]
struct DrawBodyEffects {
    menu_focus_request: Option<MenuFocusRequest>,
    export_scale_requested: Option<ExportScale>,
    visual_export_scale_requested: Option<ExportScale>,
    visual_export_background_requested: Option<Option<VisualColor>>,
    visual_export_cancel_requested: Option<VisualExportCancelFence>,
    visual_export_cancel_pointer_down: bool,
    open_viewer_requested: bool,
    active_menu_scope: Option<MenuFocusScope>,
    visual_selection: Option<VisualSelectionState>,
    visual_canvas_editing_allowed: bool,
    visual_structural_editing_allowed: bool,
    visual_asset_picker_requested: Option<VisualAssetImportFence>,
    recent_panel: Option<RecentPanelEffects>,
}

#[derive(Default)]
struct RecentPanelEffects {
    open: Option<bool>,
    search: Option<String>,
}

struct PendingRootEffects {
    root_pass: ViewportPassProvenance,
    host: HostIdentity,
    change_host: bool,
    native_viewport: Option<egui::ViewportInfo>,
    owner_generation: u64,
    owner_viewport_id: egui::ViewportId,
    owner_pass: Option<ViewportPassProvenance>,
    owner_pass_accepted: bool,
    child_viewport_id: Option<egui::ViewportId>,
    viewport_callback_observed: bool,
    close_dialog: bool,
    close_viewport: bool,
    os_close_requested: bool,
    focus_viewport: bool,
    canvas_focus_requested: Option<egui::Id>,
    source_focus_requested: Option<egui::Id>,
    menu_focus_request: Option<MenuFocusRequest>,
    export_scale_requested: Option<ExportScale>,
    visual_export_scale_requested: Option<ExportScale>,
    visual_export_background_requested: Option<Option<VisualColor>>,
    visual_export_cancel_requested: Option<VisualExportCancelFence>,
    visual_export_cancel_pointer_down: bool,
    menu_scope_observed: bool,
    active_menu_scope: Option<MenuFocusScope>,
    visual_selection: Option<VisualSelectionState>,
    visual_canvas_editing_allowed: bool,
    visual_structural_editing_allowed: bool,
    visual_asset_picker_requested: Option<VisualAssetImportFence>,
    open_viewer_requested: bool,
    mutation: Option<PendingMutation>,
    document_intent: Option<DocumentIntent>,
    document_action_fence: Option<DocumentActionFence>,
    recent_panel: Option<RecentPanelEffects>,
}

#[derive(Clone)]
struct VisualExportCancelFence {
    host: HostIdentity,
    document: RenderDocumentIdentity,
    revision: u64,
    owner_generation: u64,
    owner_viewport_id: egui::ViewportId,
}

#[derive(Clone, Copy)]
struct RootOwnerPassProvenance {
    host: HostIdentity,
    root_pass: ViewportPassProvenance,
    owner_generation: u64,
    owner_viewport_id: egui::ViewportId,
    owner_pass: Option<ViewportPassProvenance>,
    owner_pass_accepted: bool,
    viewport_callback_observed: bool,
}

impl RootOwnerPassProvenance {
    fn from_effects(effects: &PendingRootEffects) -> Self {
        Self {
            host: effects.host,
            root_pass: effects.root_pass,
            owner_generation: effects.owner_generation,
            owner_viewport_id: effects.owner_viewport_id,
            owner_pass: effects.owner_pass,
            owner_pass_accepted: effects.owner_pass_accepted,
            viewport_callback_observed: effects.viewport_callback_observed,
        }
    }

    fn is_current(self, ctx: &egui::Context, owner_generation: u64, host: HostIdentity) -> bool {
        self.host == host
            && self.owner_generation == owner_generation
            && self.owner_pass_accepted
            && self.viewport_callback_observed
            && self.root_pass.was_retained(ctx)
            && match self.owner_pass {
                Some(pass) => {
                    self.owner_viewport_id == pass.viewport_id
                        && pass.viewport_id == viewport_id()
                        && pass.was_retained(ctx)
                }
                None => self.owner_viewport_id == egui::ViewportId::ROOT,
            }
    }
}

fn encode_and_stage_visual_export(
    snapshot: DiagramVisualSnapshot,
    fence: DocumentActionFence,
    format: DiagramExportFormat,
    options: DiagramVisualExportOptions,
    destination: ResolvedExportDestination,
) -> Result<StagedVisualExport, String> {
    let artifact = match format {
        DiagramExportFormat::Svg => snapshot.encode_svg(options),
        DiagramExportFormat::Png => snapshot.encode_png(options),
        DiagramExportFormat::Mermaid => {
            return Err("visual documents cannot be exported as Mermaid source".into());
        }
    }
    .map_err(|error| format!("could not encode visual export: {error}"))?;
    if artifact.revision() != fence.revision {
        return Err("visual export snapshot revision does not match its accepted action".into());
    }
    let revision = artifact.revision();
    let dimensions = artifact.dimensions();
    let warnings = artifact.warnings();
    let staged = stage_export_unoccupied(destination, format, artifact.bytes())
        .map_err(|error| error.to_string())?;
    Ok(StagedVisualExport {
        fence,
        format,
        revision,
        dimensions,
        warnings,
        staged,
    })
}

fn visual_export_warning_summary(warnings: DiagramExportWarnings) -> String {
    let mut parts = Vec::new();
    if warnings.glyph_substitutions() != 0 {
        parts.push(format!(
            "{} glyph substitutions",
            warnings.glyph_substitutions()
        ));
    }
    if warnings.visual_asset_placeholders() != 0 {
        parts.push(format!(
            "{} image placeholders",
            warnings.visual_asset_placeholders()
        ));
    }
    if warnings.visual_truncated_text_items() != 0 {
        parts.push(format!(
            "{} shortened text items",
            warnings.visual_truncated_text_items()
        ));
    }
    if warnings.visual_simplified_dash_paths() != 0 {
        parts.push(format!(
            "{} simplified dashed paths",
            warnings.visual_simplified_dash_paths()
        ));
    }
    if warnings.visual_connector_route_fallbacks() != 0 {
        parts.push(format!(
            "{} connector route fallbacks",
            warnings.visual_connector_route_fallbacks()
        ));
    }
    parts.join(", ")
}

#[derive(Clone)]
struct DocumentActionFence {
    host: HostIdentity,
    document: RenderDocumentIdentity,
    revision: u64,
    owner_generation: u64,
    owner_viewport_id: egui::ViewportId,
    root_pass: ViewportPassProvenance,
    owner_pass: Option<ViewportPassProvenance>,
}

struct PendingViewerEffects {
    root_pass: ViewportPassProvenance,
    host: HostIdentity,
    change_host: bool,
    native_viewport: Option<egui::ViewportInfo>,
    owner_generation: u64,
    viewer_generation: u64,
    callback_viewport_id: egui::ViewportId,
    callback_pass: Option<ViewportPassProvenance>,
    callback_pass_accepted: bool,
    embedded_root_layer: Option<egui::LayerId>,
    embedded_root_focused_id: Option<egui::Id>,
    close_requested: bool,
    native_close_requested: bool,
    canvas_focus_requested: Option<egui::Id>,
    focus_viewer: bool,
}

pub(crate) struct DiagramStudioDialog {
    pub(crate) open: bool,
    owner_host: UtilityHostState,
    viewer_host: UtilityHostState,
    show_root_after_host_change: bool,
    #[cfg(all(windows, not(test)))]
    picker_owner: Option<(egui::Context, Option<NativeViewportIdentity>)>,
    owner_generation: u64,
    viewport_bootstrap_pending: bool,
    focus_viewport: bool,
    focus_source: bool,
    close_pending: bool,
    pending_root_effects: Option<PendingRootEffects>,
    deferred_canvas_focus: Option<(egui::ViewportId, egui::Id)>,
    deferred_source_focus: Option<(egui::ViewportId, egui::Id)>,
    deferred_viewer_canvas_focus: Option<(egui::ViewportId, egui::Id)>,
    deferred_menu_focus: Option<(egui::ViewportId, MenuFocusRequest)>,
    active_menu_scope: Option<MenuFocusScope>,
    documents: DiagramDocumentController,
    viewer_open: bool,
    viewer_bootstrap_pending: bool,
    viewer_focus_pending: bool,
    viewer_generation: u64,
    viewer_embedded_root_layer: Option<egui::LayerId>,
    viewer_embedded_root_focused_id: Option<egui::Id>,
    pending_viewer_effects: Option<PendingViewerEffects>,
    pending_markdown_source: Option<DiagramMarkdownSource>,
    default_directory: PathBuf,
    recent_index_path: Option<PathBuf>,
    recent_index: Option<RecentDiagramIndex>,
    recent_index_loaded: bool,
    recent_index_error: Option<String>,
    recent_index_status: Option<String>,
    recent_search: String,
    recent_panel_open: bool,
    view_mode: ViewMode,
    source_preview_split_ratio: f32,
    inspector_canvas_split_ratio: f32,
    export_scale: ExportScale,
    visual_export_background: Option<VisualColor>,
    cache: Option<RevisionCache>,
    visual_projection_cache: Option<VisualProjectionCache>,
    visual_asset_textures: HashMap<VisualAssetId, VisualAssetTexture>,
    visual_asset_pixels: HashMap<VisualAssetId, DecodedVisualAsset>,
    visual_asset_errors: HashMap<VisualAssetId, String>,
    visual_asset_document: Option<RenderDocumentIdentity>,
    visual_asset_revision: Option<u64>,
    visual_asset_owner_generation: u64,
    visual_asset_work: Option<PendingVisualAssetWork>,
    completed_visual_asset_import: Option<CompletedVisualAssetImport>,
    completed_visual_export: Option<StagedVisualExport>,
    drawio_work: Option<PendingDrawioWork>,
    drawio_review: Option<DrawioWorkflowReview>,
    renderer: Option<RendererService>,
    camera: Camera,
    viewer_camera: Camera,
    visual_viewer_fit_follow: bool,
    visual_viewer_fit_size: Option<egui::Vec2>,
    visual_viewer_minimap_drag: Option<MinimapDrag>,
    visual_editor_minimap_drag: Option<MinimapDrag>,
    render_document: RenderDocumentIdentity,
    render_options: RenderOptions,
    generic_preview: GenericPreviewStatus,
    generic_texture: Option<GenericPreviewTexture>,
    selection: Option<DiagramElement>,
    selected_nodes: BTreeSet<NodeId>,
    property_draft: Option<PropertyDraft>,
    visual_create_shape: VisualPrimitiveShape,
    visual_membership_target: Option<VisualObjectId>,
    visual_selection: Option<VisualObjectId>,
    visual_selected_objects: BTreeSet<VisualObjectId>,
    visual_selected_preset: Option<crate::diagram::VisualPresetId>,
    visual_property_draft: Option<VisualPropertyDraft>,
    visual_gesture: Option<VisualGesture>,
    visual_canvas_press_authorized: Option<bool>,
    drag: Option<DragState>,
    message: Option<StudioNotice>,
    #[cfg(test)]
    test_source_editor_id: Option<egui::Id>,
    #[cfg(test)]
    test_source_editor_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_source_editor_enabled: bool,
    #[cfg(test)]
    test_canvas_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_canvas_id: Option<egui::Id>,
    #[cfg(test)]
    test_action_rects: Vec<(String, egui::Rect, egui::Rect)>,
    #[cfg(test)]
    test_visual_group_enabled: bool,
    #[cfg(test)]
    test_action_focus_ids: Vec<(String, egui::Id)>,
    #[cfg(test)]
    test_discard_button_id: Option<egui::Id>,
    #[cfg(test)]
    test_body_rendered: bool,
    #[cfg(test)]
    test_callback_id: Option<egui::ViewportId>,
    #[cfg(test)]
    test_callback_class: Option<egui::ViewportClass>,
    #[cfg(test)]
    test_viewer_callback_id: Option<egui::ViewportId>,
    #[cfg(test)]
    test_viewer_callback_class: Option<egui::ViewportClass>,
    #[cfg(test)]
    test_viewer_body_rendered: bool,
    #[cfg(test)]
    test_viewer_canvas_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_viewer_canvas_id: Option<egui::Id>,
    #[cfg(test)]
    test_viewer_generic_preview_drawn: bool,
    #[cfg(test)]
    test_viewer_memory_focused_id: Option<egui::Id>,
    #[cfg(test)]
    test_viewer_render_key: Option<RenderKey>,
    #[cfg(test)]
    test_viewer_visual_projection_items: usize,
    #[cfg(test)]
    test_viewer_visual_items_painted: usize,
    #[cfg(test)]
    test_viewer_visual_items_omitted: usize,
    #[cfg(test)]
    test_application_input_allowed: bool,
    #[cfg(test)]
    test_child_focused: bool,
    #[cfg(test)]
    test_request_child_discard: bool,
    #[cfg(test)]
    test_child_discard_requested: bool,
    #[cfg(test)]
    test_request_viewer_discard: bool,
    #[cfg(test)]
    test_viewer_discard_requested: bool,
    #[cfg(test)]
    test_pending_mutation: Option<PendingMutation>,
    #[cfg(test)]
    test_pending_document_intent: Option<DocumentIntent>,
    #[cfg(test)]
    test_next_file_picker_result: Option<Option<PathBuf>>,
    #[cfg(test)]
    test_file_picker_calls: usize,
    #[cfg(test)]
    test_scene_build_count: usize,
    #[cfg(test)]
    test_visual_projection_build_count: usize,
    #[cfg(test)]
    test_visual_items_painted: usize,
    #[cfg(test)]
    test_visual_items_omitted: usize,
    #[cfg(test)]
    test_visual_canvas_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_marquee_order: Option<(usize, usize, egui::Rect)>,
    #[cfg(test)]
    test_visual_canvas_id: Option<egui::Id>,
    #[cfg(test)]
    test_action_enabled: Vec<(String, bool)>,
    #[cfg(test)]
    test_visual_scroll_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_generic_preview_drawn: bool,
    #[cfg(test)]
    test_preview_status_text: Option<String>,
    #[cfg(test)]
    test_generic_canvas_placeholder: Option<String>,
    #[cfg(test)]
    test_native_inspector_visible: bool,
    #[cfg(test)]
    test_picker_source_change: Option<String>,
    #[cfg(test)]
    test_picker_visual_change: Option<String>,
    #[cfg(test)]
    test_picker_host_change: bool,
    #[cfg(test)]
    test_visual_export_scale_requested: Option<ExportScale>,
    #[cfg(test)]
    test_picker_visual_label_change: Option<String>,
}

pub(crate) struct DiagramMarkdownSource {
    pub(crate) origin: DiagramSourceCopyOrigin,
    pub(crate) source: String,
}

impl Default for DiagramStudioDialog {
    fn default() -> Self {
        Self::new_with_directory(PathBuf::from("."))
    }
}

impl DiagramStudioDialog {
    pub(crate) fn new(root: &crate::platform::app_data::AppDataRoot) -> Self {
        Self::new_with_recent_index(
            root.path().to_path_buf(),
            Some(root.path().join("diagram-recents.json")),
        )
    }

    fn new_with_directory(default_directory: PathBuf) -> Self {
        Self::new_with_recent_index(default_directory, None)
    }

    fn new_with_recent_index(
        default_directory: PathBuf,
        recent_index_path: Option<PathBuf>,
    ) -> Self {
        Self {
            open: false,
            owner_generation: 0,
            owner_host: UtilityHostState::popped_out(),
            viewer_host: UtilityHostState::popped_out(),
            show_root_after_host_change: false,
            #[cfg(all(windows, not(test)))]
            picker_owner: None,
            viewport_bootstrap_pending: false,
            focus_viewport: false,
            focus_source: false,
            close_pending: false,
            pending_root_effects: None,
            deferred_canvas_focus: None,
            deferred_source_focus: None,
            deferred_viewer_canvas_focus: None,
            deferred_menu_focus: None,
            active_menu_scope: None,
            documents: DiagramDocumentController::new(),
            viewer_open: false,
            viewer_bootstrap_pending: false,
            viewer_focus_pending: false,
            viewer_generation: 0,
            viewer_embedded_root_layer: None,
            viewer_embedded_root_focused_id: None,
            pending_viewer_effects: None,
            pending_markdown_source: None,
            default_directory,
            recent_index_path,
            recent_index: None,
            recent_index_loaded: false,
            recent_index_error: None,
            recent_index_status: None,
            recent_search: String::new(),
            recent_panel_open: false,
            view_mode: ViewMode::Split,
            source_preview_split_ratio: DEFAULT_SOURCE_PREVIEW_RATIO,
            inspector_canvas_split_ratio: DEFAULT_INSPECTOR_CANVAS_RATIO,
            export_scale: ExportScale::default(),
            visual_export_background: None,
            cache: None,
            visual_projection_cache: None,
            visual_asset_textures: HashMap::new(),
            visual_asset_pixels: HashMap::new(),
            visual_asset_errors: HashMap::new(),
            visual_asset_document: None,
            visual_asset_revision: None,
            visual_asset_owner_generation: 0,
            visual_asset_work: None,
            completed_visual_asset_import: None,
            completed_visual_export: None,
            drawio_work: None,
            drawio_review: None,
            renderer: None,
            camera: Camera::default(),
            viewer_camera: Camera::default(),
            visual_viewer_fit_follow: true,
            visual_viewer_fit_size: None,
            visual_viewer_minimap_drag: None,
            visual_editor_minimap_drag: None,
            render_document: RenderDocumentIdentity::fresh(),
            render_options: RenderOptions::default(),
            generic_preview: GenericPreviewStatus::Idle,
            generic_texture: None,
            selection: None,
            selected_nodes: BTreeSet::new(),
            property_draft: None,
            visual_create_shape: VisualPrimitiveShape::Rectangle,
            visual_membership_target: None,
            visual_selection: None,
            visual_selected_objects: BTreeSet::new(),
            visual_selected_preset: None,
            visual_property_draft: None,
            visual_gesture: None,
            visual_canvas_press_authorized: None,
            drag: None,
            message: None,
            #[cfg(test)]
            test_source_editor_id: None,
            #[cfg(test)]
            test_source_editor_rect: None,
            #[cfg(test)]
            test_source_editor_enabled: false,
            #[cfg(test)]
            test_canvas_rect: None,
            #[cfg(test)]
            test_canvas_id: None,
            #[cfg(test)]
            test_action_rects: Vec::new(),
            #[cfg(test)]
            test_visual_group_enabled: false,
            #[cfg(test)]
            test_action_focus_ids: Vec::new(),
            #[cfg(test)]
            test_discard_button_id: None,
            #[cfg(test)]
            test_body_rendered: false,
            #[cfg(test)]
            test_callback_id: None,
            #[cfg(test)]
            test_callback_class: None,
            #[cfg(test)]
            test_viewer_callback_id: None,
            #[cfg(test)]
            test_viewer_callback_class: None,
            #[cfg(test)]
            test_viewer_body_rendered: false,
            #[cfg(test)]
            test_viewer_canvas_rect: None,
            #[cfg(test)]
            test_viewer_canvas_id: None,
            #[cfg(test)]
            test_viewer_generic_preview_drawn: false,
            #[cfg(test)]
            test_viewer_memory_focused_id: None,
            #[cfg(test)]
            test_viewer_render_key: None,
            #[cfg(test)]
            test_viewer_visual_projection_items: 0,
            #[cfg(test)]
            test_viewer_visual_items_painted: 0,
            #[cfg(test)]
            test_viewer_visual_items_omitted: 0,
            #[cfg(test)]
            test_application_input_allowed: false,
            #[cfg(test)]
            test_child_focused: false,
            #[cfg(test)]
            test_request_child_discard: false,
            #[cfg(test)]
            test_child_discard_requested: false,
            #[cfg(test)]
            test_request_viewer_discard: false,
            #[cfg(test)]
            test_viewer_discard_requested: false,
            #[cfg(test)]
            test_pending_mutation: None,
            #[cfg(test)]
            test_pending_document_intent: None,
            #[cfg(test)]
            test_next_file_picker_result: None,
            #[cfg(test)]
            test_file_picker_calls: 0,
            #[cfg(test)]
            test_scene_build_count: 0,
            #[cfg(test)]
            test_visual_projection_build_count: 0,
            #[cfg(test)]
            test_visual_items_painted: 0,
            #[cfg(test)]
            test_visual_items_omitted: 0,
            #[cfg(test)]
            test_visual_canvas_rect: None,
            #[cfg(test)]
            test_marquee_order: None,
            #[cfg(test)]
            test_visual_canvas_id: None,
            #[cfg(test)]
            test_action_enabled: Vec::new(),
            #[cfg(test)]
            test_visual_scroll_rect: None,
            #[cfg(test)]
            test_generic_preview_drawn: false,
            #[cfg(test)]
            test_preview_status_text: None,
            #[cfg(test)]
            test_generic_canvas_placeholder: None,
            #[cfg(test)]
            test_native_inspector_visible: false,
            #[cfg(test)]
            test_picker_source_change: None,
            #[cfg(test)]
            test_picker_visual_change: None,
            #[cfg(test)]
            test_picker_host_change: false,
            #[cfg(test)]
            test_visual_export_scale_requested: None,
            #[cfg(test)]
            test_picker_visual_label_change: None,
        }
    }

    /// Explicit opener; repeated user opens focus the existing child.
    pub(crate) fn open(&mut self) {
        if !self.ensure_blank_after_close() {
            return;
        }
        if !self.open {
            self.owner_generation = self.owner_generation.wrapping_add(1);
            self.viewport_bootstrap_pending = true;
        }
        self.open = true;
        if self.documents.pending_transition().is_none() {
            self.close_pending = false;
        }
        self.focus_viewport = true;
        self.focus_source = true;
        self.deferred_canvas_focus = None;
        self.deferred_source_focus = None;
        self.deferred_viewer_canvas_focus = None;
        self.deferred_menu_focus = None;
        self.active_menu_scope = None;
    }

    /// Pinned-panel maintenance may show the child but does not steal focus.
    pub(crate) fn ensure_open(&mut self) {
        // Passive maintenance must not resurrect a document accepted as
        // closed. A later explicit opener creates the next blank session.
        if !self.documents.has_active_document() {
            return;
        }
        if !self.open {
            self.owner_generation = self.owner_generation.wrapping_add(1);
            self.viewport_bootstrap_pending = true;
        }
        self.open = true;
    }

    pub(super) fn take_show_root_after_host_change(&mut self) -> bool {
        std::mem::take(&mut self.show_root_after_host_change)
    }

    pub(crate) fn needs_viewport_bootstrap(&self) -> bool {
        self.viewport_bootstrap_pending
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        (self.open && self.owner_host.presentation() == UtilityPresentation::Popout)
            || (self.viewer_open && self.viewer_host.presentation() == UtilityPresentation::Popout)
            || self.close_pending
            || self.viewer_bootstrap_pending
            || self.pending_viewer_effects.is_some()
            || self.visual_asset_work.as_ref().is_some_and(|work| matches!(&work.kind, VisualAssetWorkKind::Export))
            || self.completed_visual_export.is_some()
            || self.drawio_work.is_some()
            || self.drawio_review.is_some()
    }

    pub(super) fn docked_owner_layer(&self) -> Option<egui::LayerId> {
        (self.open && self.owner_host.presentation() == UtilityPresentation::Docked)
            .then(|| diagram_host_layer("diagram-studio"))
    }

    pub(super) fn docked_owner_owns_root_input(&self, ctx: &egui::Context) -> bool {
        self.docked_owner_layer().is_some_and(|layer| {
            focused_on_layer(ctx, layer).is_some()
                && !active_pointer_position(ctx).is_some_and(|position| {
                    ctx.layer_id_at(position)
                        .is_some_and(|hit_layer| hit_layer != layer)
                })
        })
    }

    pub(super) fn embedded_viewer_layer_if_root(&self) -> Option<egui::LayerId> {
        if self.open && self.viewer_open {
            self.viewer_embedded_root_layer
        } else {
            None
        }
    }

    pub(super) fn embedded_viewer_owns_root_input(&self, ctx: &egui::Context) -> bool {
        if !self.open || !self.viewer_open {
            return false;
        }
        let Some(layer) = self.viewer_embedded_root_layer else {
            return false;
        };
        let has_focus = if self.viewer_host.presentation() == UtilityPresentation::Docked {
            focused_on_layer(ctx, layer).is_some()
        } else {
            self.viewer_embedded_root_focused_id
                .is_some_and(|id| ctx.memory(|memory| memory.focused()) == Some(id))
        };
        has_focus
            && !active_pointer_position(ctx).is_some_and(|position| {
                ctx.layer_id_at(position)
                    .is_some_and(|hit_layer| hit_layer != layer)
            })
    }

    pub(crate) fn request_close(&mut self) {
        if self.open {
            self.close_pending = true;
        }
    }

    /// Opens or focuses Diagram Studio with an exact Mermaid source copy from
    /// another editor, using the same dirty-document transition as other
    /// replacements.
    pub(crate) fn open_source_copy(
        &mut self,
        source: DiagramSourceCopy,
    ) -> Option<TransitionOutcome> {
        self.open();
        if !self.open {
            return None;
        }
        match self.documents.request_source_copy(source) {
            Ok(outcome) => {
                self.finish_transition(outcome, false);
                self.focus_viewport = true;
                self.focus_source = true;
                Some(outcome)
            }
            Err(error) => {
                self.show_current_document_error("open copied Mermaid source", error);
                None
            }
        }
    }

    pub(crate) fn take_markdown_source_handoff(&mut self) -> Option<DiagramMarkdownSource> {
        self.pending_markdown_source.take()
    }

    pub(crate) fn active_source_copy_origin(&self) -> Option<DiagramSourceCopyOrigin> {
        self.documents.source_copy_origin().cloned()
    }

    pub(crate) fn pending_source_copy_origin(&self) -> Option<DiagramSourceCopyOrigin> {
        self.documents.pending_source_copy_origin().cloned()
    }

    fn ensure_blank_after_close(&mut self) -> bool {
        if self.documents.has_active_document() {
            return true;
        }
        match self.documents.request_new() {
            Ok(TransitionOutcome::Applied) => {
                self.reset_document_presentation();
                true
            }
            Ok(TransitionOutcome::DecisionRequired | TransitionOutcome::Cancelled) => {
                self.message = Some(StudioNotice::error(
                    "Resolve the pending document action before reopening.",
                ));
                false
            }
            Err(error) => {
                self.message = Some(StudioNotice::error(format!(
                    "Could not open a new diagram: {error}"
                )));
                false
            }
        }
    }

    fn accept_close(&mut self) {
        self.owner_generation = self.owner_generation.wrapping_add(1);
        self.open = false;
        self.close_pending = false;
        self.viewport_bootstrap_pending = false;
        self.viewer_open = false;
        self.viewer_bootstrap_pending = false;
        self.viewer_focus_pending = false;
        self.viewer_generation = self.viewer_generation.wrapping_add(1);
        self.viewer_embedded_root_layer = None;
        self.viewer_embedded_root_focused_id = None;
        self.focus_viewport = false;
        self.focus_source = false;
        self.deferred_canvas_focus = None;
        self.deferred_source_focus = None;
        self.deferred_viewer_canvas_focus = None;
        self.deferred_menu_focus = None;
        self.active_menu_scope = None;
        self.pending_viewer_effects = None;
        self.viewer_camera = Camera::default();
        self.visual_viewer_fit_follow = true;
        self.visual_viewer_fit_size = None;
        self.visual_viewer_minimap_drag = None;
        self.visual_editor_minimap_drag = None;
        self.reset_document_presentation();
    }

    fn cancel_transients(&mut self) {
        self.drag = None;
        self.property_draft = None;
    }

    #[cfg(test)]
    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_input_allowed: bool,
    ) {
        self.show_on_root(
            ctx,
            first_ui_pass,
            application_input_allowed,
            true,
            root_input_allowed,
        );
    }

    pub(super) fn show_on_root(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_visible: bool,
        root_input_allowed: bool,
    ) {
        #[cfg(feature = "qa-framebuffer-capture")]
        self.show_impl(
            ctx,
            first_ui_pass,
            application_input_allowed,
            root_visible,
            root_input_allowed,
            None,
        );
        #[cfg(not(feature = "qa-framebuffer-capture"))]
        self.show_impl(
            ctx,
            first_ui_pass,
            application_input_allowed,
            root_visible,
            root_input_allowed,
        );
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(super) fn show_with_observation(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_visible: bool,
        root_input_allowed: bool,
    ) -> Option<DiagramViewportObservation> {
        let mut observation = None;
        self.show_impl(
            ctx,
            first_ui_pass,
            application_input_allowed,
            root_visible,
            root_input_allowed,
            Some(&mut observation),
        );
        if let Some(observation) = observation.as_mut() {
            if observation.is_immediate_target() {
                let viewport_id = observation.child_viewport_id;
                observation.completed_frame_nr = Some(ctx.cumulative_frame_nr_for(viewport_id));
                observation.completed_pass_nr = Some(ctx.cumulative_pass_nr_for(viewport_id));
            }
            observation.renderer_returned_at_unix_ms = Some(unix_millis());
        }
        observation
    }

    fn show_impl(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_visible: bool,
        root_input_allowed: bool,
        #[cfg(feature = "qa-framebuffer-capture")] mut observation_slot: Option<
            &mut Option<DiagramViewportObservation>,
        >,
    ) {
        self.refresh_cache();
        self.poll_visual_asset_work();
        self.release_retired_drawio_slot();
        if !self.open {
            self.pending_root_effects = None;
            return;
        }

        let mut pending = None;
        let mut mutation_authorized = false;
        let mut document_intent_authorized = false;
        let mut child_pass_accepted = false;
        let mut close_requested = false;
        let mut os_close_requested = false;
        let mut focus_requested = false;
        let mut canvas_focus_requested = None;
        let mut source_focus_requested = None;
        let mut menu_focus_request = None;
        let mut export_scale_requested = None;
        let mut visual_export_scale_requested = None;
        let mut visual_export_background_requested = None;
        let mut visual_export_cancel_requested = None;
        let mut visual_export_cancel_pointer_down = false;
        let mut active_menu_scope = None;
        let mut visual_selection_requested = None;
        let mut visual_canvas_editing_allowed = false;
        let mut visual_structural_editing_allowed = false;
        let mut visual_asset_picker_requested = None;
        let mut open_viewer_requested = false;
        let mut document_intent = None;
        let mut recent_panel = None;
        let mut body_rendered = false;
        let mut child_modal_open = false;
        let mut child_focused = false;
        let mut callback_class = egui::ViewportClass::Root;
        let mut child_viewport_id = None;
        let mut owner_viewport_id = egui::ViewportId::ROOT;
        let mut owner_pass = None;
        let mut native_viewport = None;
        let mut owner_pass_accepted = false;
        let owner_generation = self.owner_generation;
        let owner_host = self.owner_host.identity();
        let mut change_host = false;
        let can_change_host = self.can_change_owner_host();
        let action_document = self.render_document.clone();
        let action_revision = self
            .documents
            .session()
            .map(DiagramSession::revision)
            .or_else(|| self.documents.visual_revision())
            .unwrap_or_default();
        self.test_begin_frame();
        let host = DiagramHost {
            title: "Diagram Studio",
            key: "diagram-studio",
            viewport: viewport_id(),
            builder: viewport_builder(),
            size: DEFAULT_VIEWPORT_SIZE,
            minimum: MIN_VIEWPORT_SIZE,
        };
        let docked_close = host.show(ctx, owner_host.host, root_visible || self.close_pending, root_input_allowed, |child, class| {
            callback_class = class;
            let independent = class == egui::ViewportClass::Immediate;
            if independent {
                native_viewport = Some(child.input(|input| input.viewport().clone()));
            }
            let child_context = child.ctx().clone();
            child_viewport_id = Some(child_context.viewport_id());
            owner_viewport_id = if independent {
                child_context.viewport_id()
            } else {
                ctx.viewport_id()
            };
            #[cfg(windows)]
            child.input_mut(|input| {
                reserve_windows_system_menu_alt_space(&mut input.events);
                reserve_windows_system_menu_alt_space(&mut input.raw.events);
            });
            let current_pass = child.current_pass_index() == 0;
            let modal_open = child.memory(|memory| memory.top_modal_layer().is_some());
            let (focused, close_requested_by_os) = child.input(|input| {
                (
                    input.focused,
                    independent && input.viewport().close_requested(),
                )
            });
            child_focused = focused;
            child_modal_open = modal_open;
            let pass_authorized =
                first_ui_pass && current_pass && !ctx.will_discard() && !child.will_discard();
            let surface_input_allowed = if independent {
                application_input_allowed && child_context.viewport_id() == viewport_id()
            } else {
                root_input_allowed && child_context.viewport_id() == ctx.viewport_id()
            };
            let embedded_viewer_focused = !independent
                && child_context.viewport_id() == egui::ViewportId::ROOT
                && self.embedded_viewer_owns_root_input(&child_context);
            let visible_body = independent || (child.is_visible() && !child.is_sizing_pass());
            let child_input_allowed = surface_input_allowed
                && focused
                && !modal_open
                && (class != egui::ViewportClass::EmbeddedWindow || !embedded_viewer_focused);
            let actions_allowed = pass_authorized && child_input_allowed && visible_body;
            let keyboard_actions_allowed = actions_allowed
                && !embedded_viewer_focused
                && (class != egui::ViewportClass::Root
                    || focused_on_layer(&child_context, child.layer_id()).is_some());
            let source_focus_allowed =
                pass_authorized && surface_input_allowed && !modal_open && !embedded_viewer_focused;
            focus_requested = self.focus_viewport
                && pass_authorized
                && surface_input_allowed
                && visible_body
                && !modal_open
                && (independent || class == egui::ViewportClass::Root);

            let popup_open = egui::Popup::is_any_open(child);
            let keyboard_close = if keyboard_actions_allowed {
                let (escape_pressed, close_shortcut) = child.input_mut(|input| {
                    (
                        !popup_open && input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                        input.consume_key(egui::Modifiers::COMMAND, egui::Key::W),
                    )
                });
                if escape_pressed {
                    let gesture_active = self.visual_gesture.take().is_some();
                    let press_authorized =
                        self.visual_canvas_press_authorized.take().unwrap_or(false);
                    let owned_gesture = gesture_active || press_authorized;
                    !owned_gesture
                } else {
                    close_shortcut
                }
            } else {
                false
            };
            let keyboard_document_intent = if keyboard_actions_allowed
                && !popup_open
                && self.documents.pending_transition().is_none()
            {
                child.input_mut(|input| {
                    if input.consume_key(egui::Modifiers::COMMAND, egui::Key::N) {
                        Some(DocumentIntent::New)
                    } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::O) {
                        Some(DocumentIntent::Open)
                    } else if input.consume_key(
                        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                        egui::Key::S,
                    ) {
                        Some(DocumentIntent::SaveAs)
                    } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::S) {
                        Some(DocumentIntent::Save)
                    } else {
                        None
                    }
                })
            } else {
                None
            };
            document_intent = keyboard_document_intent;
            os_close_requested = close_requested_by_os;
            if pass_authorized && (close_requested_by_os || keyboard_close || self.close_pending) {
                close_requested = true;
                document_intent = Some(DocumentIntent::Close);
            }

            // Restore an accepted source-suggestion focus transfer before the
            // TextEdit is drawn, so the next input event reaches the editor.
            if child.is_visible()
                && !child.is_sizing_pass()
                && pass_authorized
                && surface_input_allowed
                && !modal_open
                && !close_requested
                && let Some((target_viewport, id)) = self.deferred_source_focus
                && target_viewport == child_context.viewport_id()
            {
                self.deferred_source_focus = None;
                child.memory_mut(|memory| memory.request_focus(id));
            }

            if independent {
                egui::CentralPanel::default().show(child, |ui| {
                    body_rendered = ui.is_visible() && !ui.is_sizing_pass();
                    let (control, _) = presentation_button(
                        ui,
                        owner_host.host,
                        actions_allowed && !close_requested && can_change_host,
                    );
                    self.record_host_action("owner_presentation", ui, &control);
                    change_host = control.clicked();
                    let effects = self.draw_body(
                        ui,
                        actions_allowed,
                        keyboard_actions_allowed,
                        close_requested,
                        &mut pending,
                        &mut document_intent,
                        &mut canvas_focus_requested,
                        &mut source_focus_requested,
                        source_focus_allowed,
                    );
                    menu_focus_request = effects.menu_focus_request;
                    export_scale_requested = effects.export_scale_requested;
                    visual_export_scale_requested = effects.visual_export_scale_requested;
                    visual_export_background_requested = effects.visual_export_background_requested;
                    visual_export_cancel_requested = effects.visual_export_cancel_requested;
                    visual_export_cancel_pointer_down = effects.visual_export_cancel_pointer_down;
                    open_viewer_requested = effects.open_viewer_requested;
                    active_menu_scope = effects.active_menu_scope;
                    visual_selection_requested = effects.visual_selection;
                    visual_canvas_editing_allowed = effects.visual_canvas_editing_allowed;
                    visual_structural_editing_allowed = effects.visual_structural_editing_allowed;
                    visual_asset_picker_requested = effects.visual_asset_picker_requested;
                    recent_panel = effects.recent_panel;
                });
            } else {
                body_rendered = child.is_visible() && !child.is_sizing_pass();
                let (control, _) = presentation_button(
                    child,
                    owner_host.host,
                    actions_allowed && !close_requested && can_change_host,
                );
                self.record_host_action("owner_presentation", child, &control);
                change_host = control.clicked();
                let effects = self.draw_body(
                    child,
                    actions_allowed,
                    keyboard_actions_allowed,
                    close_requested,
                    &mut pending,
                    &mut document_intent,
                    &mut canvas_focus_requested,
                    &mut source_focus_requested,
                    source_focus_allowed,
                );
                menu_focus_request = effects.menu_focus_request;
                export_scale_requested = effects.export_scale_requested;
                visual_export_scale_requested = effects.visual_export_scale_requested;
                visual_export_background_requested = effects.visual_export_background_requested;
                visual_export_cancel_requested = effects.visual_export_cancel_requested;
                visual_export_cancel_pointer_down = effects.visual_export_cancel_pointer_down;
                open_viewer_requested = effects.open_viewer_requested;
                active_menu_scope = effects.active_menu_scope;
                visual_selection_requested = effects.visual_selection;
                visual_canvas_editing_allowed = effects.visual_canvas_editing_allowed;
                visual_structural_editing_allowed = effects.visual_structural_editing_allowed;
                visual_asset_picker_requested = effects.visual_asset_picker_requested;
                recent_panel = effects.recent_panel;
            }
            if body_rendered
                && pass_authorized
                && surface_input_allowed
                && !modal_open
                && !close_requested
            {
                if let Some((target_viewport, id)) = self.deferred_canvas_focus
                    && target_viewport == child_context.viewport_id()
                {
                    self.deferred_canvas_focus = None;
                    child.memory_mut(|memory| memory.request_focus(id));
                }
                if let Some((target_viewport, request)) = self.deferred_menu_focus
                    && target_viewport == child_context.viewport_id()
                {
                    let target_id = match request.destination {
                        MenuFocusDestination::Item(id)
                            if active_menu_scope.as_ref().is_some_and(|scope| {
                                scope.popup_id == request.popup_id
                                    && !scope.sizing_pass
                                    && scope.item_ids.contains(&id)
                            }) =>
                        {
                            Some(id)
                        }
                        MenuFocusDestination::Trigger(id)
                            if active_menu_scope.is_none()
                                && !egui::Popup::is_id_open(&child_context, request.popup_id) =>
                        {
                            Some(id)
                        }
                        _ => None,
                    };
                    self.deferred_menu_focus = None;
                    if let Some(id) = target_id {
                        child.memory_mut(|memory| memory.request_focus(id));
                    }
                }
            }
            #[cfg(test)]
            if std::mem::take(&mut self.test_request_child_discard) {
                child.request_discard("Diagram Studio test child pass discard");
                self.test_child_discard_requested = true;
            }
            mutation_authorized = actions_allowed && !child.will_discard() && !ctx.will_discard();
            document_intent_authorized = mutation_authorized
                || (close_requested && (os_close_requested || self.close_pending));
            child_pass_accepted = pass_authorized && !child.will_discard() && !ctx.will_discard();
            owner_pass_accepted = child_pass_accepted;
            if independent && child_pass_accepted {
                owner_pass = Some(ViewportPassProvenance {
                    viewport_id: child_context.viewport_id(),
                    frame_nr: child.cumulative_frame_nr(),
                    pass_nr: child.cumulative_pass_nr(),
                });
            }
            #[cfg(test)]
            {
                self.test_callback_id = Some(child_context.viewport_id());
                self.test_callback_class = Some(class);
                self.test_application_input_allowed = application_input_allowed;
                self.test_child_focused = focused;
                self.test_body_rendered = body_rendered;
            }
            #[cfg(feature = "qa-framebuffer-capture")]
            if let Some(slot) = observation_slot.as_deref_mut() {
                let (visible, minimized, occluded, focused) = child.input(|input| {
                    let viewport = input.viewport();
                    (
                        viewport.visible(),
                        viewport.minimized,
                        viewport.occluded,
                        viewport.focused,
                    )
                });
                let cached = self.cache.as_ref();
                let status = cached
                    .map(|cache| cache.status.clone())
                    .or_else(|| self.documents.session().map(DiagramSession::status))
                    .unwrap_or(SessionStatus {
                        preview: PreviewState::NoPreview,
                        diagnostic: None,
                    });
                *slot = Some(DiagramViewportObservation {
                    child_viewport_id: child_context.viewport_id(),
                    callback_class: class,
                    callback_frame_nr: child.cumulative_frame_nr(),
                    callback_pass_nr: child.cumulative_pass_nr(),
                    callback_pass_index: child.current_pass_index(),
                    visible,
                    minimized,
                    occluded,
                    focused,
                    body_rendered,
                    still_open: self.open,
                    application_input_allowed,
                    child_modal_open,
                    pixels_per_point: child.pixels_per_point(),
                    native_pixels_per_point: child.native_pixels_per_point(),
                    zoom_factor: child.zoom_factor(),
                    session_revision: self.documents.session().map_or(0, DiagramSession::revision),
                    rendered_scene_revision: cached
                        .and_then(|cache| cache.scene.as_ref().map(|_| cache.revision)),
                    status,
                    callback_observed_at_unix_ms: unix_millis(),
                    completed_frame_nr: None,
                    completed_pass_nr: None,
                    renderer_returned_at_unix_ms: None,
                });
            }
        });

        if docked_close && owner_pass_accepted {
            close_requested = true;
            document_intent = Some(DocumentIntent::Close);
            document_intent_authorized = true;
            pending = None;
        }

        if first_ui_pass {
            let viewport_callback_observed = child_viewport_id.is_some();
            let owner_effects_accepted = child_pass_accepted && !ctx.will_discard();
            let root_pass = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
            let document_action_fence = document_intent
                .as_ref()
                .filter(|intent| intent.requires_action_fence())
                .map(|_| DocumentActionFence {
                    host: owner_host,
                    document: action_document,
                    revision: action_revision,
                    owner_generation,
                    owner_viewport_id,
                    root_pass,
                    owner_pass,
                });
            self.pending_root_effects = Some(PendingRootEffects {
                root_pass,
                host: owner_host,
                change_host: owner_effects_accepted && !close_requested && change_host,
                native_viewport,
                owner_generation,
                owner_viewport_id,
                owner_pass,
                owner_pass_accepted,
                child_viewport_id: child_pass_accepted.then_some(child_viewport_id).flatten(),
                viewport_callback_observed,
                close_dialog: close_requested && child_pass_accepted,
                close_viewport: (close_requested || document_intent.is_some())
                    && child_pass_accepted
                    && callback_class == egui::ViewportClass::Immediate,
                os_close_requested,
                focus_viewport: focus_requested && child_pass_accepted && !close_requested,
                canvas_focus_requested: (child_pass_accepted && !close_requested)
                    .then_some(canvas_focus_requested)
                    .flatten(),
                source_focus_requested: (child_pass_accepted && !close_requested)
                    .then_some(source_focus_requested)
                    .flatten(),
                menu_focus_request: (child_pass_accepted && !close_requested && body_rendered)
                    .then_some(menu_focus_request)
                    .flatten(),
                export_scale_requested: (child_pass_accepted && !close_requested && body_rendered)
                    .then_some(export_scale_requested)
                    .flatten(),
                visual_export_scale_requested: (child_pass_accepted
                    && !close_requested
                    && body_rendered)
                    .then_some(visual_export_scale_requested)
                    .flatten(),
                visual_export_background_requested: (child_pass_accepted
                    && !close_requested
                    && body_rendered)
                    .then_some(visual_export_background_requested)
                    .flatten(),
                visual_export_cancel_requested: (child_pass_accepted
                    && !close_requested
                    && body_rendered)
                    .then_some(visual_export_cancel_requested)
                    .flatten(),
                visual_export_cancel_pointer_down: child_pass_accepted
                    && !close_requested
                    && body_rendered
                    && visual_export_cancel_pointer_down,
                menu_scope_observed: child_pass_accepted && body_rendered,
                active_menu_scope: (child_pass_accepted && body_rendered)
                    .then_some(active_menu_scope)
                    .flatten(),
                visual_selection: (child_pass_accepted && body_rendered)
                    .then_some(visual_selection_requested)
                    .flatten(),
                visual_canvas_editing_allowed: child_pass_accepted
                    && body_rendered
                    && visual_canvas_editing_allowed,
                visual_structural_editing_allowed: child_pass_accepted
                    && body_rendered
                    && visual_structural_editing_allowed,
                visual_asset_picker_requested: (child_pass_accepted
                    && body_rendered
                    && !close_requested)
                    .then_some(visual_asset_picker_requested)
                    .flatten(),
                open_viewer_requested: owner_effects_accepted
                    && !close_requested
                    && open_viewer_requested,
                mutation: (mutation_authorized && child_pass_accepted)
                    .then_some(pending)
                    .flatten(),
                document_intent: (child_pass_accepted && document_intent_authorized)
                    .then_some(document_intent)
                    .flatten(),
                document_action_fence,
                recent_panel: (child_pass_accepted && body_rendered)
                    .then_some(recent_panel)
                    .flatten(),
            });
        }
        #[cfg(test)]
        {
            self.test_body_rendered = body_rendered;
            self.test_child_focused = child_focused;
            let _ = child_modal_open;
        }
        if self.viewer_open {
            self.show_viewer(
                ctx,
                first_ui_pass,
                application_input_allowed,
                root_visible,
                root_input_allowed,
            );
        } else if first_ui_pass {
            self.pending_viewer_effects = None;
        }
        let _ = (body_rendered, child_focused, child_modal_open);
    }

    fn show_viewer(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_visible: bool,
        root_input_allowed: bool,
    ) {
        let owner_generation = self.owner_generation;
        let viewer_generation = self.viewer_generation;
        let viewer_host = self.viewer_host.identity();
        let mut change_host = false;
        let mut callback_viewport_id = None;
        let mut callback_pass = None;
        let mut native_viewport = None;
        let mut callback_pass_accepted = false;
        let mut embedded_root_layer = None;
        let mut embedded_root_focused_id = None;
        let mut native_close_requested = false;
        let mut close_requested = false;
        let mut focus_requested = false;
        let mut canvas_focus_requested = None;
        let mut body_rendered = false;
        #[cfg(test)]
        {
            self.test_viewer_callback_id = None;
            self.test_viewer_callback_class = None;
            self.test_viewer_body_rendered = false;
            self.test_viewer_generic_preview_drawn = false;
            self.test_viewer_memory_focused_id = None;
        }

        let host = DiagramHost {
            title: "Diagram Viewer",
            key: "diagram-viewer",
            viewport: viewer_viewport_id(),
            builder: viewer_viewport_builder(),
            size: VIEWER_VIEWPORT_SIZE,
            minimum: MIN_VIEWER_VIEWPORT_SIZE,
        };
        let docked_close = host.show(ctx, viewer_host.host, root_visible, root_input_allowed, |child, class| {
            let child_context = child.ctx().clone();
            let actual_viewport_id = child_context.viewport_id();
            callback_viewport_id = Some(actual_viewport_id);
            let independent = class == egui::ViewportClass::Immediate
                && actual_viewport_id == viewer_viewport_id();
            if independent {
                native_viewport = Some(child.input(|input| input.viewport().clone()));
            }
            let current_pass = child.current_pass_index() == 0;
            let modal_open = child.memory(|memory| memory.top_modal_layer().is_some());
            let (focused, close_requested_by_os) = child.input(|input| {
                (
                    input.focused,
                    independent && input.viewport().close_requested(),
                )
            });
            let pass_authorized =
                first_ui_pass && current_pass && !ctx.will_discard() && !child.will_discard();
            let surface_input_allowed = if independent {
                application_input_allowed
            } else {
                root_input_allowed
                    && child_context.viewport_id() == ctx.viewport_id()
                    && ctx.viewport_id() == egui::ViewportId::ROOT
            };
            let actions_allowed = pass_authorized
                && surface_input_allowed
                && !modal_open
                && !close_requested_by_os
                && (!independent || focused)
                && (independent || (child.is_visible() && !child.is_sizing_pass()));
            focus_requested = self.viewer_focus_pending
                && pass_authorized
                && surface_input_allowed
                && !modal_open
                && (independent
                    || (class == egui::ViewportClass::Root
                        && child.is_visible()
                        && !child.is_sizing_pass()));
            native_close_requested = close_requested_by_os;
            close_requested = close_requested_by_os;

            if independent {
                egui::CentralPanel::default().show(child, |ui| {
                    body_rendered = ui.is_visible() && !ui.is_sizing_pass();
                    let (control, _) = presentation_button(ui, viewer_host.host, actions_allowed);
                    self.record_host_action("viewer_presentation", ui, &control);
                    change_host = control.clicked();
                    close_requested |=
                        self.draw_viewer_body(ui, actions_allowed, &mut canvas_focus_requested);
                });
            } else {
                body_rendered = child.is_visible() && !child.is_sizing_pass();
                if actual_viewport_id == egui::ViewportId::ROOT {
                    embedded_root_layer = Some(child.layer_id());
                }
                let (control, _) = presentation_button(child, viewer_host.host, actions_allowed);
                self.record_host_action("viewer_presentation", child, &control);
                change_host = control.clicked();
                close_requested |=
                    self.draw_viewer_body(child, actions_allowed, &mut canvas_focus_requested);
                if focus_requested && canvas_focus_requested.is_none() {
                    canvas_focus_requested = Some(control.id);
                }
            }

            if body_rendered
                && pass_authorized
                && surface_input_allowed
                && !modal_open
                && !close_requested
                && let Some((target_viewport, id)) = self.deferred_viewer_canvas_focus
                && target_viewport == actual_viewport_id
            {
                self.deferred_viewer_canvas_focus = None;
                child.memory_mut(|memory| memory.request_focus(id));
            }

            if !independent && actual_viewport_id == egui::ViewportId::ROOT {
                embedded_root_layer = Some(child.layer_id());
                let pointer_position = child.input(|input| input.pointer.interact_pos());
                let pointer_active = child
                    .input(|input| input.pointer.any_pressed() || input.pointer.any_released());
                let pointer_over_viewer = pointer_position.is_some_and(|position| {
                    child_context.layer_id_at(position) == Some(child.layer_id())
                });
                embedded_root_focused_id = if pointer_active && pointer_over_viewer {
                    canvas_focus_requested
                        .or_else(|| focused_on_layer(&child_context, child.layer_id()))
                } else if pointer_active {
                    None
                } else {
                    let focused_id = child.memory(|memory| memory.focused());
                    self.viewer_embedded_root_focused_id
                        .filter(|viewer_id| focused_id == Some(*viewer_id))
                };
            }

            #[cfg(test)]
            if std::mem::take(&mut self.test_request_viewer_discard) {
                child.request_discard("Diagram viewer test child pass discard");
                self.test_viewer_discard_requested = true;
            }

            callback_pass_accepted =
                pass_authorized && !child.will_discard() && !ctx.will_discard();
            if independent && callback_pass_accepted {
                callback_pass = Some(ViewportPassProvenance {
                    viewport_id: actual_viewport_id,
                    frame_nr: child.cumulative_frame_nr(),
                    pass_nr: child.cumulative_pass_nr(),
                });
            }
            #[cfg(test)]
            {
                self.test_viewer_callback_id = Some(actual_viewport_id);
                self.test_viewer_callback_class = Some(class);
                self.test_viewer_body_rendered = body_rendered;
                self.test_viewer_memory_focused_id = child.memory(|memory| memory.focused());
            }
        });
        close_requested |= docked_close;

        if first_ui_pass {
            self.pending_viewer_effects =
                callback_viewport_id.map(|callback_viewport_id| PendingViewerEffects {
                    root_pass: ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT),
                    owner_generation,
                    viewer_generation,
                    host: viewer_host,
                    change_host: callback_pass_accepted && !close_requested && change_host,
                    native_viewport,
                    callback_viewport_id,
                    callback_pass,
                    callback_pass_accepted,
                    embedded_root_layer,
                    embedded_root_focused_id,
                    close_requested,
                    native_close_requested,
                    canvas_focus_requested: callback_pass_accepted
                        .then_some(canvas_focus_requested)
                        .flatten(),
                    focus_viewer: focus_requested && callback_pass_accepted,
                });
        }
    }

    fn record_host_action(&mut self, name: &str, ui: &egui::Ui, response: &egui::Response) {
        #[cfg(test)]
        {
            self.test_action_rects
                .push((name.into(), response.rect, ui.clip_rect()));
            self.test_action_enabled.push((
                name.into(),
                response.enabled() && ui.is_visible() && !ui.is_sizing_pass(),
            ));
        }
        #[cfg(not(test))]
        let _ = (name, ui, response);
    }

    fn draw_viewer_body(
        &mut self,
        ui: &mut egui::Ui,
        actions_allowed: bool,
        canvas_focus_requested: &mut Option<egui::Id>,
    ) -> bool {
        if let Some(label) = self.documents.visual_label().map(str::to_owned) {
            let presentation = self.visual_projection();
            if let Some((scene, _)) = presentation.as_ref() {
                let revision = self.documents.visual_revision().unwrap_or(0);
                let pending = self.documents.pending_transition().is_some();
                self.sync_visual_asset_scope(scene, revision, pending, false);
                self.poll_visual_asset_work();
                self.upload_visual_asset_textures(ui.ctx());
            }
            let projection = presentation.map(|(_, projection)| projection);
            return self.draw_visual_viewer_body(
                ui,
                actions_allowed,
                &label,
                projection,
                canvas_focus_requested,
            );
        }
        self.ensure_generic_texture(ui.ctx());
        let Self {
            documents,
            cache,
            render_document,
            render_options,
            generic_preview,
            generic_texture,
            viewer_camera,
            #[cfg(test)]
            test_viewer_canvas_rect,
            #[cfg(test)]
            test_viewer_canvas_id,
            #[cfg(test)]
            test_viewer_generic_preview_drawn,
            #[cfg(test)]
            test_viewer_render_key,
            ..
        } = self;
        let Some(session) = documents.session() else {
            ui.label("No diagram is open.");
            return false;
        };
        let revision = session.revision();
        #[cfg(test)]
        {
            *test_viewer_render_key = Some(RenderKey::new(
                render_document.clone(),
                revision,
                *render_options,
            ));
        }
        let status = cache.as_ref().map(|cache| &cache.status);
        let graph = session.preview_graph();
        let generic_family = generic_preview_family(session.draft());
        let generic_key = generic_family
            .map(|_| RenderKey::new(render_document.clone(), revision, *render_options));
        let generic_output = generic_key
            .as_ref()
            .and_then(|key| generic_preview_output(generic_preview, key));
        let texture = generic_texture.as_ref().filter(|texture| {
            texture.key.document == *render_document && texture.key.options == *render_options
        });
        let showing_stale_generic =
            texture.is_some_and(|texture| texture.key.accepted_revision != revision);
        let generic_status = generic_key.as_ref().and_then(|key| {
            generic_preview_status_text(generic_preview, key, showing_stale_generic)
        });
        let generic_placeholder = generic_key.as_ref().map_or_else(
            || "No preview is available for this source.".to_owned(),
            |key| generic_preview_canvas_placeholder(generic_preview, key).to_owned(),
        );
        let cache = cache.as_ref();
        let scene = cache.and_then(|cache| cache.scene.as_ref());
        let mut close_requested = false;

        ui.horizontal_wrapped(|ui| {
            ui.strong("Diagram Viewer");
            ui.separator();
            let zoom_out = ui.add_enabled(actions_allowed, egui::Button::new("−"));
            if zoom_out.clicked() {
                viewer_camera.zoom = (viewer_camera.zoom / CAMERA_KEYBOARD_ZOOM_FACTOR)
                    .clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
                viewer_camera.fit_next = false;
            }
            ui.label(format!("{:.0}%", viewer_camera.zoom * 100.0));
            let zoom_in = ui.add_enabled(actions_allowed, egui::Button::new("+"));
            if zoom_in.clicked() {
                viewer_camera.zoom = (viewer_camera.zoom * CAMERA_KEYBOARD_ZOOM_FACTOR)
                    .clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
                viewer_camera.fit_next = false;
            }
            if ui
                .add_enabled(actions_allowed, egui::Button::new("Fit"))
                .clicked()
            {
                viewer_camera.fit_next = true;
            }
            if ui
                .add_enabled(actions_allowed, egui::Button::new("100%"))
                .clicked()
            {
                viewer_camera.zoom = 1.0;
                viewer_camera.fit_next = false;
            }
            close_requested = ui
                .add_enabled(actions_allowed, egui::Button::new("Close viewer"))
                .clicked();
        });

        if generic_family.is_some() {
            if let Some((color, text)) = generic_status.as_ref() {
                ui.colored_label(*color, text);
            } else if generic_output.is_some() {
                ui.label("Current rendered preview");
            }
        } else if let Some(status) = status {
            match status.preview {
                PreviewState::Current => {
                    ui.label("Current native graph preview");
                }
                PreviewState::Stale => {
                    let diagnostic = status
                        .diagnostic
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "The source has changed.".to_owned());
                    ui.colored_label(
                        egui::Color32::from_rgb(190, 104, 18),
                        format!("Stale native preview — {diagnostic}"),
                    );
                }
                PreviewState::NoPreview => {
                    let diagnostic = status
                        .diagnostic
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "No valid preview is available.".to_owned());
                    ui.colored_label(egui::Color32::from_rgb(185, 65, 65), diagnostic);
                }
            }
            if let Some(error) = cache.and_then(|cache| cache.scene_error.as_ref()) {
                ui.colored_label(egui::Color32::from_rgb(185, 65, 65), error);
            }
        }

        ui.separator();
        let mut selection = None;
        let mut selected_nodes = BTreeSet::new();
        let mut drag = None;
        let mut pending = None;
        let mut message = None;
        if generic_family.is_some() {
            #[cfg(test)]
            let mut viewer_generic_preview_drawn = false;
            #[cfg(test)]
            let mut viewer_generic_placeholder = None;
            let _rect = ui
                .scope_builder(viewer_canvas_scope_builder(), |ui| {
                    draw_generic_canvas(
                        ui,
                        texture,
                        viewer_camera,
                        actions_allowed,
                        &generic_placeholder,
                        canvas_focus_requested,
                        true,
                        #[cfg(test)]
                        test_viewer_canvas_id,
                        #[cfg(test)]
                        &mut viewer_generic_preview_drawn,
                        #[cfg(test)]
                        &mut viewer_generic_placeholder,
                    )
                })
                .inner;
            #[cfg(test)]
            {
                *test_viewer_canvas_rect = Some(_rect);
                *test_viewer_generic_preview_drawn = viewer_generic_preview_drawn;
            }
        } else {
            let mut unused_focus = None;
            let _rect = ui
                .scope_builder(viewer_canvas_scope_builder(), |ui| {
                    draw_canvas(
                        ui,
                        graph,
                        scene,
                        status.is_some_and(|status| status.preview == PreviewState::Current),
                        revision,
                        viewer_camera,
                        &mut selection,
                        &mut selected_nodes,
                        &mut drag,
                        actions_allowed,
                        CanvasInteraction::Viewer,
                        &mut pending,
                        &mut message,
                        &mut unused_focus,
                        #[cfg(test)]
                        test_viewer_canvas_id,
                    )
                })
                .inner;
            *canvas_focus_requested = unused_focus.or(*canvas_focus_requested);
            #[cfg(test)]
            {
                *test_viewer_canvas_rect = Some(_rect);
            }
        }
        close_requested
    }

    fn draw_visual_viewer_body(
        &mut self,
        ui: &mut egui::Ui,
        actions_allowed: bool,
        label: &str,
        projection: Option<Result<Arc<VisualSceneProjection>, VisualProjectionError>>,
        canvas_focus_requested: &mut Option<egui::Id>,
    ) -> bool {
        let mut camera = self.viewer_camera;
        let mut close_requested = false;
        ui.horizontal_wrapped(|ui| {
            ui.strong("Diagram Viewer");
            ui.separator();
            let zoom_out = ui.add_enabled(actions_allowed, egui::Button::new("−"));
            #[cfg(test)]
            self.test_action_rects.push((
                "visual_viewer_zoom_out".into(),
                zoom_out.rect,
                ui.clip_rect(),
            ));
            if zoom_out.clicked() {
                camera.zoom = (camera.zoom / CAMERA_KEYBOARD_ZOOM_FACTOR)
                    .clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
                camera.fit_next = false;
                self.visual_viewer_fit_follow = false;
            }
            ui.label(format!("{:.0}%", camera.zoom * 100.0));
            let zoom_in = ui.add_enabled(actions_allowed, egui::Button::new("+"));
            #[cfg(test)]
            self.test_action_rects.push((
                "visual_viewer_zoom_in".into(),
                zoom_in.rect,
                ui.clip_rect(),
            ));
            if zoom_in.clicked() {
                camera.zoom = (camera.zoom * CAMERA_KEYBOARD_ZOOM_FACTOR)
                    .clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
                camera.fit_next = false;
                self.visual_viewer_fit_follow = false;
            }
            let fit = ui.add_enabled(actions_allowed, egui::Button::new("Fit"));
            #[cfg(test)]
            self.test_action_rects
                .push(("visual_viewer_fit".into(), fit.rect, ui.clip_rect()));
            if fit.clicked() {
                camera.fit_next = true;
                self.visual_viewer_fit_follow = true;
            }
            let zoom_100 = ui.add_enabled(actions_allowed, egui::Button::new("100%"));
            #[cfg(test)]
            self.test_action_rects.push((
                "visual_viewer_zoom_100".into(),
                zoom_100.rect,
                ui.clip_rect(),
            ));
            if zoom_100.clicked() {
                camera.zoom = 1.0;
                camera.fit_next = false;
                self.visual_viewer_fit_follow = false;
            }
            close_requested = ui
                .add_enabled(actions_allowed, egui::Button::new("Close viewer"))
                .clicked();
        });
        ui.separator();
        ui.label("Read-only visual document preview");
        if let Some((id, error)) = self
            .visual_asset_errors
            .iter()
            .min_by_key(|(id, _)| id.as_str())
        {
            ui.colored_label(
                egui::Color32::from_rgb(185, 65, 65),
                format!("Image {} is unavailable: {error}", &id.as_str()[..8]),
            );
        }
        let width = ui.available_width();
        let height = ui.available_height();
        let desired = egui::vec2(
            if width.is_finite() {
                width.clamp(1.0, 1_000_000.0)
            } else {
                240.0
            },
            if height.is_finite() {
                height.clamp(1.0, 1_000_000.0)
            } else {
                300.0
            },
        );
        let (canvas_response, painter) =
            ui.allocate_painter(desired, egui::Sense::click_and_drag());
        let canvas = canvas_response.rect;
        let painter = painter.with_clip_rect(canvas);
        let canvas_bounds = projection
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .and_then(|projection| projection.bounds);
        if (camera.fit_next
            || (self.visual_viewer_fit_follow
                && self.visual_viewer_minimap_drag.is_none()
                && self.visual_viewer_fit_size != Some(canvas.size())))
            && let Some(bounds) = canvas_bounds
        {
            fit_camera(&mut camera, bounds, canvas);
            self.visual_viewer_fit_size = Some(canvas.size());
        }
        let minimap_rect = visual_minimap_screen_rect(canvas, painter.clip_rect());
        let minimap_response = minimap_rect.map(|rect| {
            ui.interact(
                rect,
                ui.make_persistent_id(("diagram-visual-viewer-minimap", canvas_response.id)),
                egui::Sense::click_and_drag(),
            )
        });
        #[cfg(test)]
        if let Some(rect) = minimap_rect {
            self.test_action_rects
                .push(("visual_viewer_minimap".into(), rect, ui.clip_rect()));
        }
        let before_minimap = camera;
        let minimap_interacting = handle_visual_minimap_drag(
            ui,
            minimap_rect,
            &mut camera,
            &mut self.visual_viewer_minimap_drag,
            actions_allowed,
            self.viewer_host.identity(),
            self.viewer_generation,
            || {
                let (rect, projection) = (minimap_rect?, projection.as_ref()?.as_ref().ok()?);
                minimap_layout_for_camera(
                    projection,
                    canvas,
                    painter.clip_rect(),
                    rect,
                    before_minimap,
                )
            },
        );
        let pointer_navigation = handle_visual_canvas_camera_input(
            ui,
            &canvas_response,
            painter.clip_rect(),
            minimap_rect,
            &mut camera,
            actions_allowed && !minimap_interacting,
        );
        let keyboard_navigation = if actions_allowed
            && !minimap_interacting
            && !minimap_response
                .as_ref()
                .is_some_and(egui::Response::hovered)
        {
            viewer_canvas_shortcuts(ui, canvas_response.id, &mut camera)
        } else {
            false
        };
        if pointer_navigation || keyboard_navigation {
            self.visual_viewer_fit_follow = false;
        }
        if actions_allowed
            && canvas_response.clicked()
            && !minimap_interacting
            && !minimap_rect.is_some_and(|rect| {
                ui.ctx()
                    .input(|input| input.pointer.interact_pos())
                    .is_some_and(|p| rect.contains(p))
            })
        {
            *canvas_focus_requested = Some(canvas_response.id);
        }
        let mut paint_report = VisualPaintReport::default();
        #[cfg(test)]
        let projected_item_count = projection
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .map_or(0, |projection| projection.items.len());
        match projection.as_ref() {
            Some(Ok(projection)) => {
                paint_report = paint_visual_projection(
                    &painter,
                    projection,
                    canvas,
                    camera,
                    &BTreeSet::new(),
                    &self.visual_asset_textures,
                    &self.visual_asset_errors,
                );
            }
            Some(Err(error)) => {
                ui.painter()
                    .rect_filled(canvas, 4.0, egui::Color32::from_gray(248));
                ui.painter().text(
                    canvas.center(),
                    egui::Align2::CENTER_CENTER,
                    error.to_string(),
                    egui::FontId::proportional(14.0),
                    egui::Color32::DARK_RED,
                );
            }
            None => {
                ui.painter().text(
                    canvas.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(18.0),
                    ui.visuals().text_color(),
                );
            }
        }
        if let (Some(rect), Some(Ok(projection))) = (minimap_rect, projection.as_ref())
            && let Some(minimap) =
                minimap_layout_for_camera(projection, canvas, painter.clip_rect(), rect, camera)
        {
            paint_visual_minimap(&painter, rect, &minimap);
        }
        if let Some(Ok(projection)) = projection.as_ref()
            && (projection.simplified_dash_paths > 0
                || projection.truncated_text_items > 0
                || projection.connector_route_fallbacks > 0
                || paint_report.omitted_items > 0
                || paint_report.suppressed_text_lines > 0)
        {
            ui.weak(format!(
                "{} large dashed outline(s) simplified; {} connector route(s) shown in orange fallback; {} text item(s) shortened, {} small text line(s) omitted; {} item(s) omitted to keep the preview responsive.",
                projection.simplified_dash_paths,
                projection.connector_route_fallbacks,
                projection.truncated_text_items,
                paint_report.suppressed_text_lines,
                paint_report.omitted_items
            ));
        }
        #[cfg(test)]
        {
            self.test_viewer_canvas_rect = Some(canvas);
            self.test_viewer_canvas_id = Some(canvas_response.id);
            self.test_viewer_visual_projection_items = projected_item_count;
            self.test_viewer_visual_items_painted = paint_report.painted_items;
            self.test_viewer_visual_items_omitted = paint_report.omitted_items;
        }
        self.viewer_camera = camera;
        close_requested
    }

    fn can_change_owner_host(&self) -> bool {
        // These workflows retain an owner-viewport picker or work fence.
        self.documents.pending_transition().is_none()
            && self.drawio_work.is_none()
            && self.drawio_review.is_none()
            && self.visual_asset_work.is_none()
            && self.completed_visual_asset_import.is_none()
            && self.completed_visual_export.is_none()
    }

    pub(crate) fn on_ui_frame_complete(&mut self, ctx: &egui::Context) -> bool {
        let viewer_effects = self.pending_viewer_effects.take();
        let Some(effects) = self.pending_root_effects.take() else {
            cancel_minimap_drag(&mut self.visual_editor_minimap_drag, &mut self.camera);
            cancel_minimap_drag(
                &mut self.visual_viewer_minimap_drag,
                &mut self.viewer_camera,
            );
            if viewer_effects
                .as_ref()
                .is_some_and(|effects| effects.native_close_requested)
            {
                ctx.send_viewport_cmd_to(viewer_viewport_id(), egui::ViewportCommand::CancelClose);
            }
            return false;
        };
        let retained_pass = effects.root_pass.was_retained(ctx);
        if !retained_pass {
            cancel_minimap_drag(&mut self.visual_editor_minimap_drag, &mut self.camera);
            cancel_minimap_drag(
                &mut self.visual_viewer_minimap_drag,
                &mut self.viewer_camera,
            );
            if let Some(export) = self.completed_visual_export.as_ref() {
                ctx.request_repaint_of(export.fence.owner_viewport_id);
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
            let discarded_gesture_commit = effects.mutation.as_ref().is_some_and(|mutation| {
                matches!(mutation, PendingMutation::VisualGestureTransaction { .. })
            });
            if self.visual_gesture.is_some() || discarded_gesture_commit {
                self.visual_gesture = None;
                self.visual_canvas_press_authorized = None;
            }
            if effects.os_close_requested {
                ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::CancelClose);
            }
            if viewer_effects
                .as_ref()
                .is_some_and(|effects| effects.native_close_requested)
            {
                ctx.send_viewport_cmd_to(viewer_viewport_id(), egui::ViewportCommand::CancelClose);
            }
            if let Some(work) = self.drawio_work.as_ref() {
                ctx.request_repaint_of(egui::ViewportId::ROOT);
                if work.scope.owner_viewport_id != egui::ViewportId::ROOT {
                    ctx.request_repaint_of(work.scope.owner_viewport_id);
                }
                if effects.owner_viewport_id != egui::ViewportId::ROOT {
                    ctx.request_repaint_of(effects.owner_viewport_id);
                }
            }
            return false;
        }
        let owner_pass_current = self.owner_pass_is_current(&effects, ctx);
        finish_minimap_drag(
            &mut self.visual_editor_minimap_drag,
            &mut self.camera,
            owner_pass_current
                && !effects.change_host
                && !effects.close_dialog
                && !effects.close_viewport
                && !effects.os_close_requested
                && effects.document_intent.is_none(),
            self.owner_host.identity(),
            self.owner_generation,
            ctx,
        );
        let viewer_current = viewer_effects.as_ref().is_some_and(|viewer| {
            viewer.root_pass == effects.root_pass
                && viewer.owner_generation == self.owner_generation
                && viewer.viewer_generation == self.viewer_generation
                && viewer.host == self.viewer_host.identity()
                && self.open
                && self.viewer_open
                && viewer.callback_pass_accepted
                && !viewer.close_requested
                && !viewer.change_host
                && if viewer.callback_viewport_id == viewer_viewport_id() {
                    viewer.callback_pass.is_some_and(|pass| {
                        pass.viewport_id == viewer_viewport_id() && pass.was_retained(ctx)
                    })
                } else {
                    viewer.callback_viewport_id == egui::ViewportId::ROOT
                        && viewer.embedded_root_layer.is_some()
                        && viewer.callback_pass.is_none()
                }
        });
        if finish_minimap_drag(
            &mut self.visual_viewer_minimap_drag,
            &mut self.viewer_camera,
            viewer_current
                && !effects.change_host
                && !effects.close_dialog
                && !effects.close_viewport
                && effects.document_intent.is_none(),
            self.viewer_host.identity(),
            self.viewer_generation,
            ctx,
        ) {
            self.visual_viewer_fit_follow = false;
        }
        let completion_pass = RootOwnerPassProvenance::from_effects(&effects);
        if owner_pass_current
            && let Some(cancel) = effects.visual_export_cancel_requested.as_ref()
            && cancel.host == self.owner_host.identity()
            && cancel.document == self.render_document
            && self.documents.visual_revision() == Some(cancel.revision)
            && cancel.owner_generation == self.owner_generation
            && cancel.owner_viewport_id == effects.owner_viewport_id
            && self.documents.pending_transition().is_none()
        {
            self.cancel_visual_export();
        }
        let accepted_owner_viewport_id = effects.owner_viewport_id;
        let owner_native = native_geometry_identity(
            "diagram-studio",
            viewport_id(),
            effects.owner_generation,
            effects.host,
        );
        let viewer_native = native_geometry_identity(
            "diagram-viewer",
            viewer_viewport_id(),
            self.viewer_generation,
            self.viewer_host.identity(),
        );
        if owner_pass_current
            && let Some(info) = effects.native_viewport.as_ref()
            && let Some(identity) = owner_native
        {
            super::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                egui::ViewportClass::Immediate,
                true,
                "Diagram Studio",
                identity,
                info,
                egui::Vec2::from(DEFAULT_VIEWPORT_SIZE),
                Some(egui::Vec2::from(MIN_VIEWPORT_SIZE)),
                None,
            );
        }
        #[cfg(all(windows, not(test)))]
        {
            self.picker_owner = if !owner_pass_current {
                None
            } else if accepted_owner_viewport_id == egui::ViewportId::ROOT {
                Some((ctx.clone(), None))
            } else {
                owner_native.map(|identity| (ctx.clone(), Some(identity)))
            };
        }
        if owner_pass_current && let Some(recent_panel) = effects.recent_panel.as_ref() {
            if let Some(open) = recent_panel.open {
                self.recent_panel_open = open;
                if open {
                    self.ensure_recent_index_loaded();
                }
            }
            if let Some(search) = recent_panel.search.as_ref() {
                self.recent_search.clone_from(search);
            }
        }
        self.process_visual_asset_picker_request(ctx, &effects, owner_pass_current);
        let mut cancel_visual_gesture = false;
        if let Some(gesture) = self.visual_gesture.as_mut() {
            let document_current = gesture.document == self.render_document
                && Some(gesture.revision) == self.documents.visual_revision();
            let owner_current = gesture.owner_generation == self.owner_generation
                && gesture.owner_viewport_id == effects.owner_viewport_id;
            let start_pass_matches = if gesture.owner_viewport_id == egui::ViewportId::ROOT {
                gesture.start_pass == effects.root_pass
            } else {
                effects.owner_pass == Some(gesture.start_pass)
            };
            if !document_current
                || !owner_current
                || effects.close_dialog
                || effects.close_viewport
                || effects.os_close_requested
                || effects.document_intent.is_some()
                || !owner_pass_current
                || !effects.visual_canvas_editing_allowed
            {
                cancel_visual_gesture = true;
            } else if !gesture.start_pass_accepted {
                if start_pass_matches && owner_pass_current && effects.owner_pass_accepted {
                    gesture.start_pass_accepted = true;
                } else {
                    cancel_visual_gesture = true;
                }
            }
        }
        if cancel_visual_gesture {
            self.visual_gesture = None;
            self.visual_canvas_press_authorized = None;
        }
        if owner_pass_current && let Some(selection) = effects.visual_selection.as_ref() {
            self.set_visual_selection(selection.clone());
        }

        let bootstrap_completed =
            self.viewport_bootstrap_pending && effects.viewport_callback_observed;
        if bootstrap_completed {
            self.viewport_bootstrap_pending = false;
        }
        if effects.menu_scope_observed {
            self.active_menu_scope = effects.active_menu_scope;
        }
        let child_viewport_id = effects.child_viewport_id;
        let repaint_child = effects.focus_viewport
            || effects.canvas_focus_requested.is_some()
            || effects.source_focus_requested.is_some()
            || effects.menu_focus_request.is_some()
            || effects.export_scale_requested.is_some()
            || effects.visual_export_scale_requested.is_some()
            || effects.visual_export_background_requested.is_some()
            || effects.visual_export_cancel_requested.is_some()
            || effects.visual_export_cancel_pointer_down
            || effects.mutation.is_some()
            || effects.close_dialog
            || effects.close_viewport
            || effects.document_intent.is_some()
            || effects.os_close_requested;
        let repaint_root = bootstrap_completed || repaint_child;

        if let Some(target_viewport) = child_viewport_id {
            if let Some(id) = effects.canvas_focus_requested {
                self.deferred_canvas_focus = Some((target_viewport, id));
            }
            if let Some(id) = effects.source_focus_requested {
                self.deferred_source_focus = Some((target_viewport, id));
                self.focus_source = false;
            }
            if let Some(request) = effects.menu_focus_request {
                self.deferred_menu_focus = Some((target_viewport, request));
            }
        }
        if let Some(scale) = effects.export_scale_requested {
            self.export_scale = scale;
        }
        if owner_pass_current && let Some(scale) = effects.visual_export_scale_requested { self.export_scale = scale; }
        if owner_pass_current && let Some(background) = effects.visual_export_background_requested { self.visual_export_background = background; }
        if effects.focus_viewport && owner_pass_current {
            if let Some(layer) = self.docked_owner_layer() {
                ctx.move_to_top(layer);
            } else {
                ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Focus);
            }
            self.focus_viewport = false;
        }
        let source_mutation_requested = effects.mutation.is_some();
        let source_cursor_request = effects
            .mutation
            .as_ref()
            .and_then(|mutation| match mutation {
                PendingMutation::ReplaceSourceWithCursor {
                    editor_id,
                    cursor_character,
                    ..
                } => Some((*editor_id, *cursor_character)),
                _ => None,
            });
        let source_cursor_mutation_requested = source_cursor_request.is_some();
        let visual_mutation_requested = effects.mutation.as_ref().is_some_and(|mutation| {
            matches!(
                mutation,
                PendingMutation::ReplaceVisualLabel { .. }
                    | PendingMutation::VisualTransaction { .. }
                    | PendingMutation::VisualAssetImport { .. }
                    | PendingMutation::VisualLayoutFailure { .. }
                    | PendingMutation::VisualGestureTransaction { .. }
                    | PendingMutation::VisualHistory { .. }
            )
        });
        let mutation_succeeded = effects.mutation.is_none_or(|mutation| {
            let gesture_capture_is_current = match &mutation {
                PendingMutation::VisualGestureTransaction {
                    owner_generation,
                    owner_viewport_id,
                    start_pass_accepted,
                    ..
                } => {
                    *owner_generation == self.owner_generation
                        && *owner_viewport_id == effects.owner_viewport_id
                        && *start_pass_accepted
                }
                PendingMutation::VisualAssetImport { fence, .. } => {
                    fence.owner_generation == self.owner_generation
                        && fence.owner_viewport_id == effects.owner_viewport_id
                        && fence.start_pass_accepted
                        && fence.document == self.render_document
                        && Some(fence.revision) == self.documents.visual_revision()
                }
                _ => true,
            };
            if visual_mutation_requested && (!owner_pass_current || !gesture_capture_is_current) {
                self.reject_stale_visual_intent()
            } else if source_cursor_mutation_requested && !owner_pass_current {
                self.message = Some(StudioNotice::error(
                    "The editor pass was discarded; choose the Mermaid suggestion again.",
                ));
                false
            } else {
                self.apply_pending(mutation)
            }
        });
        self.retire_stale_drawio_work(owner_pass_current.then_some(accepted_owner_viewport_id));
        if mutation_succeeded
            && effects.document_intent.is_none()
            && let Some((editor_id, cursor_character)) = source_cursor_request
        {
            install_text_edit_cursor(ctx, editor_id, cursor_character);
            if let Some(target_viewport) = child_viewport_id {
                self.deferred_source_focus = Some((target_viewport, editor_id));
            }
        }
        let debounce_accepted_source = source_mutation_requested && mutation_succeeded;
        let viewer_was_open = self.viewer_open;
        let mut document_action_fence = effects.document_action_fence.clone();
        let is_drawio_intent = effects
            .document_intent
            .as_ref()
            .is_some_and(DocumentIntent::is_drawio);
        if source_mutation_requested
            && mutation_succeeded
            && !is_drawio_intent
            && !matches!(effects.document_intent.as_ref(), Some(DocumentIntent::ApplyVisualTemplate(_)))
            && let Some(fence) = document_action_fence.as_mut()
            && fence.document == self.render_document
            && fence.owner_generation == self.owner_generation
        {
            fence.revision = self
                .documents
                .session()
                .map(DiagramSession::revision)
                .or_else(|| self.documents.visual_revision())
                .unwrap_or_default();
        }
        let accepted_close = mutation_succeeded
            && effects.document_intent.is_some_and(|intent| {
                self.apply_document_intent(
                    intent,
                    document_action_fence.as_ref(),
                    ctx,
                    owner_pass_current,
                )
            });
        self.publish_completed_visual_export(ctx, completion_pass, effects.close_dialog || effects.os_close_requested, accepted_close, effects.visual_export_cancel_pointer_down);
        self.poll_drawio_work(ctx, accepted_owner_viewport_id, owner_pass_current);
        #[cfg(all(windows, not(test)))]
        {
            self.picker_owner = None;
        }
        if (accepted_close
            || effects.change_host
                && owner_pass_current
                && self.open
                && self.can_change_owner_host())
            && let Some(identity) = owner_native
        {
            super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
        }
        if effects.change_host
            && owner_pass_current
            && self.open
            && !accepted_close
            && self.can_change_owner_host()
        {
            self.show_root_after_host_change |=
                transition_diagram_host(ctx, &mut self.owner_host, viewport_id());
            self.viewport_bootstrap_pending =
                self.owner_host.presentation() == UtilityPresentation::Popout;
            self.focus_viewport = true;
            self.focus_source = true;
            self.deferred_canvas_focus = None;
            self.deferred_source_focus = None;
            self.deferred_menu_focus = None;
            self.active_menu_scope = None;
            self.visual_gesture = None;
            self.visual_canvas_press_authorized = None;
        }
        let owner_closed_viewer = accepted_close && viewer_was_open;
        if self.open {
            let repaint_target = child_viewport_id.unwrap_or(egui::ViewportId::ROOT);
            self.sync_generic_preview_after_accepted_pass(
                ctx,
                repaint_target,
                debounce_accepted_source,
            );
            let completion_published = self.publish_generic_preview_completion();
            if completion_published && self.viewer_open {
                ctx.request_repaint_of(viewer_viewport_id());
            }
        }
        let owner_viewer_request_is_current =
            effects.open_viewer_requested && self.open && owner_pass_current;
        if owner_viewer_request_is_current {
            if !self.viewer_open {
                self.viewer_generation = self.viewer_generation.wrapping_add(1);
                self.viewer_embedded_root_focused_id = None;
                self.viewer_open = true;
                self.visual_viewer_fit_follow = true;
                self.visual_viewer_fit_size = None;
                self.visual_viewer_minimap_drag = None;
            }
            self.viewer_focus_pending = true;
            self.viewer_bootstrap_pending = true;
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        if effects.close_viewport && accepted_close {
            ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Close);
        } else if effects.os_close_requested && !accepted_close {
            ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::CancelClose);
        }
        if owner_closed_viewer {
            if let Some(identity) = viewer_native {
                super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            }
            ctx.send_viewport_cmd_to(viewer_viewport_id(), egui::ViewportCommand::Close);
        } else if let Some(viewer_effects) = viewer_effects {
            let same_root_pass = viewer_effects.root_pass == effects.root_pass;
            let viewer_pass_current = if viewer_effects.callback_viewport_id == viewer_viewport_id()
            {
                viewer_effects.callback_pass.is_some_and(|pass| {
                    pass.viewport_id == viewer_viewport_id() && pass.was_retained(ctx)
                })
            } else {
                viewer_effects.callback_viewport_id == egui::ViewportId::ROOT
                    && viewer_effects.embedded_root_layer.is_some()
                    && viewer_effects.callback_pass.is_none()
            };
            let viewer_effects_accepted = same_root_pass
                && viewer_effects.owner_generation == self.owner_generation
                && viewer_effects.viewer_generation == self.viewer_generation
                && viewer_effects.host == self.viewer_host.identity()
                && self.open
                && self.viewer_open
                && viewer_effects.callback_pass_accepted
                && viewer_pass_current;
            if viewer_effects_accepted {
                if let Some(identity) = viewer_native {
                    if viewer_effects.close_requested || viewer_effects.change_host {
                        super::utility_dialog_size::close_native_utility_geometry_viewport(
                            ctx, identity,
                        );
                    } else if let Some(info) = viewer_effects.native_viewport.as_ref() {
                        super::utility_dialog_size::apply_native_utility_geometry_restore(
                            ctx,
                            egui::ViewportClass::Immediate,
                            true,
                            "Diagram Viewer",
                            identity,
                            info,
                            egui::Vec2::from(VIEWER_VIEWPORT_SIZE),
                            Some(egui::Vec2::from(MIN_VIEWER_VIEWPORT_SIZE)),
                            None,
                        );
                    }
                }
                self.viewer_bootstrap_pending = false;
                self.viewer_embedded_root_layer = viewer_effects.embedded_root_layer;
                self.viewer_embedded_root_focused_id = viewer_effects.embedded_root_focused_id;
                if let Some(id) = viewer_effects.canvas_focus_requested
                    && !viewer_effects.close_requested
                {
                    self.deferred_viewer_canvas_focus =
                        Some((viewer_effects.callback_viewport_id, id));
                }
                if viewer_effects.close_requested {
                    self.viewer_open = false;
                    self.viewer_generation = self.viewer_generation.wrapping_add(1);
                    self.viewer_bootstrap_pending = false;
                    self.viewer_focus_pending = false;
                    self.viewer_embedded_root_layer = None;
                    self.viewer_embedded_root_focused_id = None;
                    self.deferred_viewer_canvas_focus = None;
                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                } else if viewer_effects.change_host {
                    self.show_root_after_host_change |=
                        transition_diagram_host(ctx, &mut self.viewer_host, viewer_viewport_id());
                    self.viewer_bootstrap_pending =
                        self.viewer_host.presentation() == UtilityPresentation::Popout;
                    self.viewer_focus_pending = true;
                    self.viewer_embedded_root_layer = None;
                    self.viewer_embedded_root_focused_id = None;
                    self.deferred_viewer_canvas_focus = None;
                } else if viewer_effects.focus_viewer
                    && viewer_effects.callback_viewport_id == viewer_viewport_id()
                {
                    ctx.send_viewport_cmd_to(viewer_viewport_id(), egui::ViewportCommand::Focus);
                    self.viewer_focus_pending = false;
                } else if viewer_effects.focus_viewer {
                    if let Some(layer) = viewer_effects.embedded_root_layer {
                        ctx.move_to_top(layer);
                        self.viewer_embedded_root_focused_id =
                            viewer_effects.canvas_focus_requested;
                        if let Some(id) = viewer_effects.canvas_focus_requested {
                            ctx.memory_mut(|memory| memory.request_focus(id));
                        }
                    }
                    self.viewer_focus_pending = false;
                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                } else if self.viewer_host.presentation() == UtilityPresentation::Popout
                    && viewer_effects.callback_viewport_id != viewer_viewport_id()
                {
                    self.viewer_focus_pending = false;
                }
                if viewer_effects.close_requested
                    && !viewer_effects.native_close_requested
                    && viewer_effects.callback_viewport_id == viewer_viewport_id()
                {
                    ctx.send_viewport_cmd_to(viewer_viewport_id(), egui::ViewportCommand::Close);
                }
            } else if viewer_effects.native_close_requested {
                ctx.send_viewport_cmd_to(viewer_viewport_id(), egui::ViewportCommand::CancelClose);
            }
        }
        if effects.close_dialog {
            self.close_pending = false;
        }
        if repaint_child && let Some(target_viewport) = child_viewport_id {
            ctx.request_repaint_of(target_viewport);
        }
        if repaint_root {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        accepted_close
    }

    fn owner_pass_is_current(&self, effects: &PendingRootEffects, ctx: &egui::Context) -> bool {
        effects.owner_generation == self.owner_generation
            && effects.host == self.owner_host.identity()
            && effects.owner_pass_accepted
            && effects.viewport_callback_observed
            && match effects.owner_pass {
                Some(pass) => {
                    effects.owner_viewport_id == pass.viewport_id
                        && pass.viewport_id == viewport_id()
                        && pass.was_retained(ctx)
                }
                None => effects.owner_viewport_id == egui::ViewportId::ROOT,
            }
    }

    #[cfg(test)]
    fn viewer_render_counters_for_test(&self) -> Option<(usize, usize, usize)> {
        self.renderer
            .as_ref()
            .map(RendererService::test_counter_snapshot)
    }

    fn apply_document_intent(
        &mut self,
        intent: DocumentIntent,
        fence: Option<&DocumentActionFence>,
        ctx: &egui::Context,
        owner_pass_current: bool,
    ) -> bool {
        if intent.requires_action_fence()
            && !fence.is_some_and(|fence| {
                self.document_action_fence_is_current(fence, ctx, owner_pass_current)
            })
        {
            self.message = Some(StudioNotice::error(
                "The Diagram document changed before that action was accepted; try again.",
            ));
            return false;
        }
        if self.documents.authoring_kind() == Some(crate::diagram::DiagramAuthoringKind::Visual)
            && matches!(
                &intent,
                DocumentIntent::ImportMermaid
                    | DocumentIntent::ApplyTemplate(_)
                    | DocumentIntent::AutoLayout
                    | DocumentIntent::Export(DiagramExportFormat::Mermaid, _)
                    | DocumentIntent::UseSourceInMarkdown(_)
            )
        {
            self.message = Some(StudioNotice::error(
                "This action is not available for visual documents.",
            ));
            return false;
        }
        if self.documents.authoring_kind() != Some(crate::diagram::DiagramAuthoringKind::Visual)
            && matches!(
                &intent,
                DocumentIntent::DrawioExport | DocumentIntent::DrawioPublishExport
            )
        {
            self.message = Some(StudioNotice::error(
                "Draw.io export is available only for an active Visual document.",
            ));
            return false;
        }
        if matches!(&intent, DocumentIntent::ApplyVisualTemplate(_))
            && self.documents.authoring_kind() != Some(crate::diagram::DiagramAuthoringKind::Visual)
        {
            self.message = Some(StudioNotice::error("Visual templates require an active Visual document."));
            return false;
        }
        if self.documents.pending_transition().is_some()
            && matches!(
                &intent,
                DocumentIntent::ImportMermaid
                    | DocumentIntent::ApplyTemplate(_)
                    | DocumentIntent::ApplyVisualTemplate(_)
                    | DocumentIntent::AutoLayout
                    | DocumentIntent::Export(_, _)
                    | DocumentIntent::UseSourceInMarkdown(_)
                    | DocumentIntent::DrawioImport
                    | DocumentIntent::DrawioExport
                    | DocumentIntent::DrawioPublishImport
                    | DocumentIntent::DrawioPublishExport
            )
        {
            self.message = Some(StudioNotice::error(
                "Resolve the pending document action before importing, changing the template, laying out, or exporting.",
            ));
            return false;
        }
        if matches!(&intent, DocumentIntent::Save | DocumentIntent::SaveAs)
            && self.documents.pending_transition().is_some()
        {
            self.message = Some(StudioNotice::error(
                "Resolve the pending document action before saving directly.",
            ));
            return false;
        }
        let mut accepted_close = false;
        match intent {
            DocumentIntent::New => match self.documents.request_new() {
                Ok(outcome) => {
                    self.finish_transition(outcome, false);
                }
                Err(error) => {
                    self.show_document_error("create a new document", None, error);
                }
            },
            DocumentIntent::NewVisual => match self.documents.request_new_visual() {
                Ok(outcome) => {
                    self.finish_transition(outcome, false);
                }
                Err(error) => {
                    self.show_document_error("create a new visual document", None, error);
                }
            },
            DocumentIntent::Open => {
                if let Some(path) = self.pick_native_document(false) {
                    match self.documents.request_open(&path) {
                        Ok(outcome) => {
                            self.finish_transition(outcome, false);
                            if outcome == TransitionOutcome::Applied {
                                self.record_recent_path(&path, "opened");
                            }
                        }
                        Err(error) => {
                            self.show_document_error("open", Some(&path), error);
                        }
                    }
                }
            }
            DocumentIntent::ConvertMermaidCopy => {
                let converted = self
                    .documents
                    .session()
                    .ok_or_else(|| "there is no active Mermaid document".to_owned())
                    .and_then(|session| {
                        mermaid_visual_copy(session).map_err(|error| error.to_string())
                    });
                let converted = match converted {
                    Ok(converted) => converted,
                    Err(error) => {
                        self.message = Some(StudioNotice::error(format!(
                            "Could not prepare a visual copy: {error}"
                        )));
                        return false;
                    }
                };
                let suggestion = self
                    .documents
                    .path()
                    .and_then(Path::file_stem)
                    .and_then(|stem| stem.to_str())
                    .map(|stem| format!("{stem}-visual.mld"))
                    .unwrap_or_else(|| "diagram-visual.mld".into());
                let Some(path) = self.pick_copy_destination(&suggestion) else {
                    return false;
                };
                let still_current = fence.is_some_and(|fence| {
                    self.document_action_fence_is_current(fence, ctx, owner_pass_current)
                });
                if !still_current {
                    self.message = Some(StudioNotice::error(
                        "The Mermaid source changed while choosing a destination; no copy was written.",
                    ));
                    return false;
                }
                match publish_visual_document_copy(&path, &converted) {
                    Ok(_) => {
                        self.message = Some(StudioNotice::info(format!(
                            "Created a visual copy at {}. The Mermaid document remains active.",
                            path.display()
                        )));
                        self.record_recent_path(&path, "copied");
                    }
                    Err(error) => {
                        self.message = Some(StudioNotice::error(format!(
                            "Could not create a visual copy at {}: {error}",
                            path.display()
                        )))
                    }
                }
            }
            DocumentIntent::OpenRecent(path) => match self.documents.request_open(&path) {
                Ok(outcome) => {
                    self.finish_transition(outcome, false);
                    if outcome == TransitionOutcome::Applied {
                        self.record_recent_path(&path, "opened");
                    }
                }
                Err(error) => self.show_document_error("open", Some(&path), error),
            },
            DocumentIntent::DuplicateRecent(source) => {
                let suggestion = source
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(|stem| format!("{stem}-copy.mld"))
                    .unwrap_or_else(|| "diagram-copy.mld".into());
                let Some(destination) = self.pick_copy_destination(&suggestion) else {
                    return false;
                };
                if !fence.is_some_and(|fence| {
                    self.document_action_fence_is_current(fence, ctx, owner_pass_current)
                }) {
                    self.message = Some(StudioNotice::error(
                        "The active document changed while choosing a destination; no duplicate was written.",
                    ));
                    return false;
                }
                match duplicate_saved_document(&source, &destination) {
                    Ok(_) => {
                        self.message = Some(StudioNotice::info(format!(
                            "Duplicated {} to {}. The active document was not changed.",
                            source.display(),
                            destination.display()
                        )));
                        self.record_recent_path(&destination, "copied");
                    }
                    Err(error) => {
                        self.show_document_error("duplicate", Some(&source), error.into())
                    }
                }
            }
            DocumentIntent::ImportMermaid => {
                if let Some(path) = self.pick_mermaid_source() {
                    match MermaidFile::new(&path)
                        .map_err(DiagramDocumentError::from)
                        .and_then(|file| self.documents.request_mermaid_import(file))
                    {
                        Ok(outcome) => {
                            self.finish_transition(outcome, false);
                        }
                        Err(error) => {
                            self.show_document_error("import Mermaid source", Some(&path), error);
                        }
                    }
                }
            }
            DocumentIntent::ApplyTemplate(template) => {
                match self.documents.request_template(template) {
                    Ok(outcome) => {
                        self.finish_transition(outcome, false);
                    }
                    Err(error) => {
                        self.show_document_error("apply template", None, error);
                    }
                }
            }
            DocumentIntent::ApplyVisualTemplate(template) => {
                match self.documents.request_visual_template(template) {
                    Ok(outcome) => { self.finish_transition(outcome, false); }
                    Err(error) => self.show_document_error("apply visual template", None, error),
                }
            }
            DocumentIntent::AutoLayout => match self.documents.auto_layout() {
                Ok(changed) => {
                    if changed {
                        self.refresh_cache();
                        self.camera.fit_next = true;
                        self.message = Some(StudioNotice::info("Automatic layout applied."));
                    }
                }
                Err(error) => {
                    self.show_current_document_error("lay out the diagram", error);
                }
            },
            DocumentIntent::Save => match self.documents.save() {
                Ok(()) => {
                    self.message = Some(StudioNotice::info("Diagram saved."));
                    if let Some(path) = self.documents.path().map(Path::to_path_buf) {
                        self.record_recent_path(&path, "saved");
                    }
                }
                Err(DiagramDocumentError::SaveAsRequired) => {
                    if let Some(path) = self.pick_native_document(true) {
                        self.save_as_to(path);
                    }
                }
                Err(error) => self.show_current_document_error("save", error),
            },
            DocumentIntent::SaveAs => {
                if let Some(path) = self.pick_native_document(true) {
                    self.save_as_to(path);
                }
            }
            DocumentIntent::Export(format, scale) => {
                if self.documents.authoring_kind() == Some(crate::diagram::DiagramAuthoringKind::Visual) {
                    if let Some(fence) = fence { self.start_visual_export(ctx, format, self.export_scale, fence.clone(), owner_pass_current); }
                } else { self.export_document(format, scale); }
            }
            DocumentIntent::DrawioImport => {
                if let Some(fence) = fence {
                    self.begin_drawio_import(fence, ctx, owner_pass_current);
                }
            }
            DocumentIntent::DrawioExport => {
                if let Some(fence) = fence {
                    self.begin_drawio_export(fence, ctx, owner_pass_current);
                }
            }
            DocumentIntent::DrawioPublishImport => {
                if let Some(fence) = fence {
                    self.publish_drawio_import_copy(fence, ctx, owner_pass_current);
                }
            }
            DocumentIntent::DrawioOpenImportCopy => {
                if let Some(fence) = fence {
                    self.open_drawio_import_copy(fence, ctx, owner_pass_current);
                }
            }
            DocumentIntent::DrawioPublishExport => {
                if let Some(fence) = fence {
                    self.publish_drawio_export_review(fence, ctx, owner_pass_current);
                }
            }
            DocumentIntent::DrawioCancel => {
                self.retire_drawio_work();
                self.message = Some(StudioNotice::info(
                    "Draw.io review cancelled; the active document and files were left unchanged.",
                ));
            }
            DocumentIntent::UseSourceInMarkdown(origin) => {
                if self.documents.source_copy_origin() != Some(&origin) {
                    self.message = Some(StudioNotice::error(
                        "This diagram is no longer linked to its originating Markdown block.",
                    ));
                } else if let Some(source) = self
                    .documents
                    .session()
                    .map(|session| session.draft().to_owned())
                {
                    self.pending_markdown_source = Some(DiagramMarkdownSource { origin, source });
                } else {
                    self.message = Some(StudioNotice::error(
                        "There is no active diagram source to send to Markdown.",
                    ));
                }
            }
            DocumentIntent::Close => {
                match self.documents.request_close() {
                    Ok(outcome) => {
                        accepted_close = self.finish_transition(outcome, true);
                    }
                    Err(error) => {
                        self.show_current_document_error("close", error);
                    }
                }
                self.close_pending = false;
            }
            DocumentIntent::ResolvePending(decision) => {
                let pending = self.documents.pending_transition().cloned();
                let path_before_save = self.documents.path().map(Path::to_path_buf);
                let dirty_before_save = self.documents.is_dirty();
                match self.documents.resolve_pending(decision) {
                    Ok(outcome) => {
                        if decision == TransitionDecision::Save
                            && dirty_before_save
                            && let Some(path) = path_before_save.as_deref()
                        {
                            self.record_recent_path(path, "saved");
                        }
                        let closes = pending == Some(PendingDiagramTransition::Close);
                        let closed = self.finish_transition(outcome, closes);
                        if outcome == TransitionOutcome::Applied
                            && let Some(PendingDiagramTransition::Open(path)) = pending.as_ref()
                        {
                            self.record_recent_path(path, "opened");
                        }
                        accepted_close = closes && closed;
                    }
                    Err(DiagramDocumentError::SaveAsRequired)
                        if decision == TransitionDecision::Save =>
                    {
                        if let Some(path) = self.pick_native_document(true) {
                            let saved_path = ensure_native_extension(path.clone());
                            accepted_close = match self.documents.resolve_pending_save_as(&path) {
                                Ok(outcome) => {
                                    self.record_recent_path(&saved_path, "saved");
                                    let closes = pending == Some(PendingDiagramTransition::Close);
                                    let closed = self.finish_transition(outcome, closes);
                                    if outcome == TransitionOutcome::Applied
                                        && let Some(PendingDiagramTransition::Open(open_path)) =
                                            pending.as_ref()
                                    {
                                        self.record_recent_path(open_path, "opened");
                                    }
                                    closes && closed
                                }
                                Err(error) => {
                                    if self.documents.path() == Some(saved_path.as_path())
                                        && !self.documents.is_dirty()
                                    {
                                        self.record_recent_path(&saved_path, "saved");
                                    }
                                    self.show_document_error("save", Some(&path), error);
                                    false
                                }
                            };
                        }
                    }
                    Err(error) => {
                        if decision == TransitionDecision::Save
                            && dirty_before_save
                            && !self.documents.is_dirty()
                            && self.documents.path() == path_before_save.as_deref()
                            && let Some(path) = path_before_save.as_deref()
                        {
                            self.record_recent_path(path, "saved");
                        }
                        self.show_current_document_error("apply the pending document action", error)
                    }
                }
            }
        }
        accepted_close
    }

    fn save_as_to(&mut self, path: PathBuf) {
        let path = ensure_native_extension(path);
        match self.documents.save_as(&path) {
            Ok(()) => {
                self.message = Some(StudioNotice::info(format!("Saved {}.", path.display())));
                self.record_recent_path(&path, "saved");
            }
            Err(error) => {
                self.show_document_error("save", Some(&path), error);
            }
        }
    }

    fn export_document(&mut self, format: DiagramExportFormat, scale: ExportScale) {
        if self.documents.pending_transition().is_some() {
            self.message = Some(StudioNotice::error(
                "Resolve the pending document action before exporting.",
            ));
            return;
        }
        let Some(session) = self.documents.session() else {
            self.message = Some(StudioNotice::error(
                "There is no active Diagram Studio document.",
            ));
            return;
        };
        let artifact = match format {
            DiagramExportFormat::Mermaid => Ok(DiagramSourceSnapshot::capture(session).encode()),
            DiagramExportFormat::Svg | DiagramExportFormat::Png => {
                if let Some(family) = generic_preview_family(session.draft()) {
                    let key = RenderKey::new(
                        self.render_document.clone(),
                        session.revision(),
                        self.render_options,
                    );
                    let Some(output) = generic_preview_output(&self.generic_preview, &key).cloned()
                    else {
                        self.message = Some(StudioNotice::error(
                            "The current Mermaid render is not ready for image export.",
                        ));
                        return;
                    };
                    if output.family != family {
                        self.message = Some(StudioNotice::error(
                            "The current Mermaid render does not match the active source family.",
                        ));
                        return;
                    }
                    let snapshot =
                        DiagramRenderedImageSnapshot::capture(session.revision(), output);
                    match format {
                        DiagramExportFormat::Svg => snapshot.encode_svg(scale.factor()),
                        DiagramExportFormat::Png => snapshot.encode_png(scale.factor()),
                        DiagramExportFormat::Mermaid => unreachable!("handled above"),
                    }
                } else {
                    let snapshot = DiagramImageSnapshot::capture(session, SceneMetrics::default());
                    match (format, snapshot) {
                        (DiagramExportFormat::Svg, Ok(snapshot)) => {
                            snapshot.encode_svg(scale.factor())
                        }
                        (DiagramExportFormat::Png, Ok(snapshot)) => {
                            snapshot.encode_png(scale.factor())
                        }
                        (_, Err(error)) => Err(error),
                        (DiagramExportFormat::Mermaid, _) => unreachable!("handled above"),
                    }
                }
            }
        };
        let artifact = match artifact {
            Ok(artifact) => artifact,
            Err(error) => {
                self.message = Some(StudioNotice::error(format!(
                    "Could not encode {} export: {error}",
                    export_format_name(format)
                )));
                return;
            }
        };
        self.message = Some(StudioNotice::info(format!(
            "{} export cancelled.",
            export_format_name(format)
        )));
        let Some(path) = self.pick_export_destination(format) else {
            return;
        };
        if let Some(native_path) = self.documents.path() {
            match same_export_destination(native_path, &path) {
                Ok(true) => {
                    self.message = Some(StudioNotice::error(format!(
                        "Cannot export over the active native Diagram document: {}",
                        path.display()
                    )));
                    return;
                }
                Ok(false) => {}
                Err(error) => {
                    self.message = Some(StudioNotice::error(format!(
                        "Could not resolve the export destination {}: {error}",
                        path.display()
                    )));
                    return;
                }
            }
        }

        let revision = artifact.revision();
        let dimensions = artifact.dimensions();
        let substitutions = artifact.warnings().glyph_substitutions();
        match artifact.publish_unoccupied(&path) {
            Ok(()) => {
                let size_message = dimensions.map_or_else(String::new, |(width, height)| {
                    format!(" ({width}×{height})")
                });
                let substitution_message = if substitutions == 0 {
                    String::new()
                } else {
                    format!("; {substitutions} glyph substitutions")
                };
                self.message = Some(StudioNotice::info(format!(
                    "Exported {} revision {revision} to {}{size_message}{substitution_message}.",
                    export_format_name(format),
                    path.display()
                )));
            }
            Err(error) => {
                self.message = Some(StudioNotice::error(error.to_string()));
            }
        }
    }

    #[cfg(not(test))]
    fn owned_file_dialog(&mut self) -> Option<rfd::FileDialog> {
        let dialog = rfd::FileDialog::new();
        #[cfg(windows)]
        {
            let hwnd = self
                .picker_owner
                .as_ref()
                .and_then(|(ctx, identity)| {
                    super::utility_dialog_size::utility_picker_hwnd(ctx, *identity)
                })
                .and_then(|hwnd| std::num::NonZeroIsize::new(hwnd as isize));
            let Some(hwnd) = hwnd else {
                self.message = Some(StudioNotice::error(
                    "The current Diagram window is not available for the file dialog. Focus it and try again.",
                ));
                return None;
            };
            return Some(dialog.set_parent(&super::add_action_dialog::NativeDialogParent(hwnd)));
        }
        #[cfg(not(windows))]
        Some(dialog)
    }

    fn start_visual_export(
        &mut self,
        ctx: &egui::Context,
        format: DiagramExportFormat,
        scale: ExportScale,
        fence: DocumentActionFence,
        owner_pass_current: bool,
    ) {
        if !matches!(format, DiagramExportFormat::Svg | DiagramExportFormat::Png) {
            self.message = Some(StudioNotice::error(
                "Visual documents can be exported as SVG or PNG.",
            ));
            return;
        }
        if self.visual_asset_work.is_some()
            || self.completed_visual_asset_import.is_some()
            || self.completed_visual_export.is_some()
            || self.drawio_work.is_some()
            || self.drawio_review.is_some()
        {
            self.message = Some(StudioNotice::error(
                "Wait for the current visual image or export operation to finish first.",
            ));
            return;
        }
        if !self.open
            || self.documents.authoring_kind() != Some(crate::diagram::DiagramAuthoringKind::Visual)
            || self.documents.pending_transition().is_some()
            || !self.document_action_fence_is_current(&fence, ctx, owner_pass_current)
        {
            self.message = Some(StudioNotice::error(
                "The visual document changed before export started; try again.",
            ));
            return;
        }

        let Some((_, projection)) = self.visual_projection() else {
            self.message = Some(StudioNotice::error(
                "There is no active visual document to export.",
            ));
            return;
        };
        let projection = match projection {
            Ok(projection) => projection,
            Err(error) => {
                self.message = Some(StudioNotice::error(format!(
                    "Could not prepare the visual export: {error}"
                )));
                return;
            }
        };
        let snapshot = DiagramVisualSnapshot::capture(fence.revision, projection);
        #[cfg(all(windows, not(test)))]
        let picker_hwnd = self.picker_owner.as_ref().and_then(|(ctx, identity)| {
            super::utility_dialog_size::utility_picker_hwnd(ctx, *identity)
        });
        let Some(path) = self.pick_export_destination(format) else {
            self.message = Some(StudioNotice::info(format!(
                "{} export cancelled.",
                export_format_name(format)
            )));
            return;
        };

        #[cfg(all(windows, not(test)))]
        if picker_hwnd.is_none()
            || self.picker_owner.as_ref().and_then(|(ctx, identity)| {
                super::utility_dialog_size::utility_picker_hwnd(ctx, *identity)
            }) != picker_hwnd
        {
            self.message = Some(StudioNotice::error(
                "The Diagram window changed while choosing an export destination; nothing was written.",
            ));
            return;
        }

        // The native picker may run a nested message loop. Validate the exact
        // accepted document action again before any worker or disk activity.
        if !self.open
            || self.documents.authoring_kind() != Some(crate::diagram::DiagramAuthoringKind::Visual)
            || self.documents.pending_transition().is_some()
            || !self.document_action_fence_is_current(&fence, ctx, owner_pass_current)
        {
            self.message = Some(StudioNotice::error(
                "The visual document changed while choosing an export destination; nothing was written.",
            ));
            return;
        }
        if let Some(native_path) = self.documents.path() {
            match same_export_destination(native_path, &path) {
                Ok(true) => {
                    self.message = Some(StudioNotice::error(format!(
                        "Cannot export over the active native Diagram document: {}",
                        path.display()
                    )));
                    return;
                }
                Ok(false) => {}
                Err(error) => {
                    self.message = Some(StudioNotice::error(format!(
                        "Could not resolve the export destination {}: {error}",
                        path.display()
                    )));
                    return;
                }
            }
        }
        let destination = match resolve_export_destination(&path, format) {
            Ok(destination) => destination,
            Err(error) => {
                self.message = Some(StudioNotice::error(format!(
                    "Could not prepare the export destination: {error}"
                )));
                return;
            }
        };

        let options = DiagramVisualExportOptions {
            scale: scale.factor(),
            background: self.visual_export_background,
        };
        let revision = fence.revision;
        let document = fence.document.clone();
        let owner_generation = fence.owner_generation;
        let owner_viewport_id = fence.owner_viewport_id;
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let wake_context = ctx.clone();
        let (sender, receiver) = sync_channel(1);

        #[cfg(test)]
        let worker: io::Result<()> = {
            let result = encode_and_stage_visual_export(
                snapshot,
                fence.clone(),
                format,
                options,
                destination,
            );
            if !worker_cancelled.load(Ordering::Acquire) {
                let _ = sender.send(VisualAssetWorkResult::Export(result));
            }
            drop(sender);
            wake_context.request_repaint_of(owner_viewport_id);
            if owner_viewport_id != egui::ViewportId::ROOT {
                wake_context.request_repaint_of(egui::ViewportId::ROOT);
            }
            Ok(())
        };

        #[cfg(not(test))]
        let worker = std::thread::Builder::new()
            .name("diagram-visual-export".into())
            .spawn(move || {
                let result = if worker_cancelled.load(Ordering::Acquire) {
                    Err("visual export was cancelled".to_owned())
                } else {
                    encode_and_stage_visual_export(snapshot, fence, format, options, destination)
                };
                if !worker_cancelled.load(Ordering::Acquire) {
                    let _ = sender.send(VisualAssetWorkResult::Export(result));
                }
                drop(sender);
                wake_context.request_repaint_of(owner_viewport_id);
                if owner_viewport_id != egui::ViewportId::ROOT {
                    wake_context.request_repaint_of(egui::ViewportId::ROOT);
                }
            });
        if let Err(error) = worker {
            self.message = Some(StudioNotice::error(format!(
                "Could not start visual export: {error}"
            )));
            return;
        }
        self.visual_asset_work = Some(PendingVisualAssetWork {
            document,
            revision,
            owner_generation,
            owner_viewport_id,
            kind: VisualAssetWorkKind::Export,
            cancelled,
            receiver,
            hydration_ids: Vec::new(),
            retired: false,
        });
        self.message = Some(StudioNotice::info(
            "Encoding visual export; it will be published after the next accepted owner pass.",
        ));
    }

    fn publish_completed_visual_export(
        &mut self,
        ctx: &egui::Context,
        completion_pass: RootOwnerPassProvenance,
        close_requested: bool,
        accepted_close: bool,
        cancel_pointer_down: bool,
    ) {
        let Some(export) = self.completed_visual_export.as_ref() else {
            return;
        };
        if !completion_pass.is_current(ctx, self.owner_generation, self.owner_host.identity()) {
            ctx.request_repaint_of(export.fence.owner_viewport_id);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            return;
        }
        if cancel_pointer_down {
            ctx.request_repaint_of(export.fence.owner_viewport_id);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            return;
        }
        let current_revision = self.documents.visual_revision();
        let current_owner = export.fence.host == self.owner_host.identity()
            && export.fence.owner_generation == self.owner_generation
            && export.fence.owner_viewport_id == completion_pass.owner_viewport_id;
        let current_document = export.fence.document == self.render_document
            && current_revision == Some(export.revision)
            && export.revision == export.fence.revision;
        if !self.open
            || !current_owner
            || !current_document
            || self.documents.authoring_kind() != Some(crate::diagram::DiagramAuthoringKind::Visual)
            || self.documents.pending_transition().is_some()
            || self.close_pending
            || close_requested
            || accepted_close
        {
            self.completed_visual_export = None;
            return;
        }

        let export = self
            .completed_visual_export
            .take()
            .expect("completed visual export was checked above");
        if let Some(native_path) = self.documents.path() {
            match same_export_destination(native_path, export.staged.destination()) {
                Ok(true) => {
                    self.message = Some(StudioNotice::error(format!(
                        "Cannot export over the active native Diagram document: {}",
                        export.staged.destination().display()
                    )));
                    return;
                }
                Ok(false) => {}
                Err(error) => {
                    self.message = Some(StudioNotice::error(format!(
                        "Could not resolve the export destination {}: {error}",
                        export.staged.destination().display()
                    )));
                    return;
                }
            }
        }

        let destination = export.staged.destination().to_path_buf();
        match export.staged.publish() {
            Ok(_) => {
                let size_message = export
                    .dimensions
                    .map_or_else(String::new, |(width, height)| {
                        format!(" ({width}×{height})")
                    });
                let warnings = visual_export_warning_summary(export.warnings);
                let warnings = if warnings.is_empty() {
                    String::new()
                } else {
                    format!("; {warnings}")
                };
                self.message = Some(StudioNotice::info(format!(
                    "Exported {} revision {} to {}{size_message}{warnings}.",
                    export_format_name(export.format),
                    export.revision,
                    destination.display()
                )));
            }
            Err(error) => {
                self.message = Some(StudioNotice::error(error.to_string()));
            }
        }
    }

    fn pick_mermaid_source(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_file_picker_calls = self.test_file_picker_calls.saturating_add(1);
            return self.test_next_file_picker_result.take().unwrap_or(None);
        }
        #[cfg(not(test))]
        {
            let picker = self.owned_file_dialog()?;
            let directory = self.picker_directory();
            let mut picker = picker.add_filter("Mermaid source (*.mmd)", &["mmd"]);
            if directory.is_dir() {
                picker = picker.set_directory(directory);
            }
            picker.pick_file()
        }
    }

    fn pick_drawio_source(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_file_picker_calls = self.test_file_picker_calls.saturating_add(1);
            if let Some(source) = self.test_picker_source_change.take() {
                let _ = self.documents.replace_source(&source);
                self.refresh_cache();
            }
            return self.test_next_file_picker_result.take().unwrap_or(None);
        }
        #[cfg(not(test))]
        {
            let picker = self.owned_file_dialog()?;
            let directory = self.picker_directory();
            let mut picker = picker.add_filter("draw.io diagram (*.drawio)", &["drawio"]);
            if directory.is_dir() {
                picker = picker.set_directory(directory);
            }
            picker.pick_file()
        }
    }

    fn pick_drawio_destination(&mut self, _suggested_name: &str) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_file_picker_calls = self.test_file_picker_calls.saturating_add(1);
            if let Some(label) = self.test_picker_visual_label_change.take() {
                let _ = self.documents.replace_visual_label(label);
                self.refresh_cache();
            }
            if let Some(source) = self.test_picker_source_change.take() {
                let _ = self.documents.replace_source(&source);
                self.refresh_cache();
            }
            return self
                .test_next_file_picker_result
                .take()
                .unwrap_or(None)
                .map(ensure_drawio_extension);
        }
        #[cfg(not(test))]
        {
            let picker = self.owned_file_dialog()?;
            let directory = self.picker_directory();
            let mut picker = picker
                .add_filter("draw.io diagram (*.drawio)", &["drawio"])
                .set_file_name(_suggested_name);
            if directory.is_dir() {
                picker = picker.set_directory(directory);
            }
            picker.save_file().map(ensure_drawio_extension)
        }
    }

    fn start_drawio_import(
        &mut self,
        ctx: &egui::Context,
        source_path: PathBuf,
        scope: DrawioWorkflowScope,
    ) {
        if self.drawio_work.is_some() || self.visual_asset_work.is_some() || self.completed_visual_export.is_some() {
            self.message = Some(StudioNotice::error(
                "Another bounded Diagram import or export is still running.",
            ));
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let wake_context = ctx.clone();
        let owner_viewport_id = scope.owner_viewport_id;
        let (sender, receiver) = sync_channel(1);
        let worker_source = source_path.clone();
        let worker = std::thread::Builder::new()
            .name("diagram-drawio-import".into())
            .spawn(move || {
                let result = if worker_cancelled.load(Ordering::Acquire) {
                    None
                } else {
                    Some(
                        read_drawio_xml_source(&worker_source)
                            .map_err(|error| DrawioImportError::Read(error.to_string()))
                            .and_then(|bytes| {
                                if worker_cancelled.load(Ordering::Acquire) {
                                    return Err(DrawioImportError::Read(
                                        "draw.io import was cancelled".into(),
                                    ));
                                }
                                import_drawio_xml(&bytes).map_err(DrawioImportError::Wire)
                            }),
                    )
                };
                if let Some(result) = result
                    && !worker_cancelled.load(Ordering::Acquire)
                {
                    let _ = sender.send(DrawioWorkResult::Import(result));
                }
                drop(sender);
                wake_context.request_repaint_of(egui::ViewportId::ROOT);
                if owner_viewport_id != egui::ViewportId::ROOT {
                    wake_context.request_repaint_of(owner_viewport_id);
                }
            });
        if let Err(error) = worker {
            self.message = Some(StudioNotice::error(format!(
                "Could not start draw.io import: {error}"
            )));
            return;
        }
        self.drawio_work = Some(PendingDrawioWork {
            scope,
            kind: DrawioWorkKind::Import { source_path },
            cancelled,
            receiver,
            retired: false,
        });
        self.message = Some(StudioNotice::info(
            "Reviewing the bounded draw.io file; the active document has not changed.",
        ));
    }

    fn start_drawio_export(
        &mut self,
        ctx: &egui::Context,
        scene: VisualScene,
        scope: DrawioWorkflowScope,
    ) {
        if self.drawio_work.is_some() || self.visual_asset_work.is_some() || self.completed_visual_export.is_some() {
            self.message = Some(StudioNotice::error(
                "Another bounded Diagram import or export is still running.",
            ));
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let wake_context = ctx.clone();
        let owner_viewport_id = scope.owner_viewport_id;
        let (sender, receiver) = sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("diagram-drawio-export".into())
            .spawn(move || {
                if !worker_cancelled.load(Ordering::Acquire) {
                    let result = export_drawio_scene(&scene);
                    if !worker_cancelled.load(Ordering::Acquire) {
                        let _ = sender.send(DrawioWorkResult::Export(result));
                    }
                }
                drop(sender);
                wake_context.request_repaint_of(egui::ViewportId::ROOT);
                if owner_viewport_id != egui::ViewportId::ROOT {
                    wake_context.request_repaint_of(owner_viewport_id);
                }
            });
        if let Err(error) = worker {
            self.message = Some(StudioNotice::error(format!(
                "Could not start draw.io export: {error}"
            )));
            return;
        }
        self.drawio_work = Some(PendingDrawioWork {
            scope,
            kind: DrawioWorkKind::Export,
            cancelled,
            receiver,
            retired: false,
        });
        self.message = Some(StudioNotice::info(
            "Preparing an export of the committed visual revision.",
        ));
    }

    fn retire_drawio_work(&mut self) {
        if let Some(work) = self.drawio_work.as_mut() {
            work.retired = true;
            work.cancelled.store(true, Ordering::Release);
        }
        self.drawio_review = None;
    }

    fn release_retired_drawio_slot(&mut self) {
        let Some(work) = self.drawio_work.as_ref() else {
            return;
        };
        if !work.retired {
            return;
        }
        match work.receiver.try_recv() {
            Ok(_) | Err(TryRecvError::Disconnected) => self.drawio_work = None,
            Err(TryRecvError::Empty) => {}
        }
    }

    fn retire_stale_drawio_work(&mut self, current_owner: Option<egui::ViewportId>) {
        let revision = self
            .documents
            .session()
            .map(DiagramSession::revision)
            .or_else(|| self.documents.visual_revision())
            .unwrap_or_default();
        let work_stale = self.drawio_work.as_ref().is_some_and(|work| {
            work.scope.document != self.render_document
                || work.scope.revision != revision
                || work.scope.owner_generation != self.owner_generation
                || self.documents.pending_transition().is_some()
                || current_owner.is_some_and(|owner| work.scope.owner_viewport_id != owner)
        });
        if work_stale && let Some(work) = self.drawio_work.as_mut() {
            work.retired = true;
            work.cancelled.store(true, Ordering::Release);
        }
        if self.drawio_review.as_ref().is_some_and(|review| {
            review.scope().document != self.render_document
                || review.scope().revision != revision
                || review.scope().owner_generation != self.owner_generation
                || current_owner.is_some_and(|owner| review.scope().owner_viewport_id != owner)
        }) {
            self.drawio_review = None;
        }
    }

    fn poll_drawio_work(
        &mut self,
        ctx: &egui::Context,
        owner_viewport_id: egui::ViewportId,
        owner_pass_current: bool,
    ) -> bool {
        self.retire_stale_drawio_work(owner_pass_current.then_some(owner_viewport_id));
        let Some(work) = self.drawio_work.as_ref() else {
            return false;
        };
        if !work.retired && !owner_pass_current {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            if work.scope.owner_viewport_id != egui::ViewportId::ROOT {
                ctx.request_repaint_of(work.scope.owner_viewport_id);
            }
            return false;
        }
        let was_retired = work.retired;
        let result = match work.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Disconnected) => None,
            Err(TryRecvError::Empty) => return false,
        };
        let Some(work) = self.drawio_work.take() else {
            return false;
        };
        if was_retired {
            return true;
        }
        let Some(result) = result else {
            self.message = Some(StudioNotice::error(
                "The draw.io worker stopped before returning a result.",
            ));
            return true;
        };
        if !work.scope.matches(self, owner_viewport_id) {
            return true;
        }
        match (work.kind, result) {
            (DrawioWorkKind::Import { source_path }, DrawioWorkResult::Import(Ok(candidate))) => {
                self.drawio_review = Some(DrawioWorkflowReview::Import {
                    scope: work.scope,
                    source_path,
                    candidate,
                    copy_path: None,
                });
                self.message = Some(StudioNotice::info(
                    "Draw.io candidate is ready for review; the source and active document are unchanged.",
                ));
            }
            (
                DrawioWorkKind::Import { source_path },
                DrawioWorkResult::Import(Err(DrawioImportError::Wire(
                    DrawioWireError::UnsupportedContent(report),
                ))),
            ) => {
                self.drawio_review = Some(DrawioWorkflowReview::ImportUnsupported {
                    scope: work.scope,
                    source_path,
                    report,
                });
                self.message = Some(StudioNotice::error(
                    "Draw.io import contains unsupported content; no candidate was created.",
                ));
            }
            (DrawioWorkKind::Import { source_path }, DrawioWorkResult::Import(Err(error))) => {
                let error = match error {
                    DrawioImportError::Read(error) => error,
                    DrawioImportError::Wire(error) => error.to_string(),
                };
                self.message = Some(StudioNotice::error(format!(
                    "Could not import {}: {error}",
                    source_path.display()
                )));
            }
            (DrawioWorkKind::Export, DrawioWorkResult::Export(Ok(result))) => {
                self.drawio_review = Some(DrawioWorkflowReview::ExportReady {
                    scope: work.scope,
                    result,
                });
                self.message = Some(StudioNotice::info(
                    "Draw.io export is ready for review; no file has been written.",
                ));
            }
            (
                DrawioWorkKind::Export,
                DrawioWorkResult::Export(Err(DrawioWireError::UnsupportedContent(report))),
            ) => {
                self.drawio_review = Some(DrawioWorkflowReview::ExportUnsupported {
                    scope: work.scope,
                    report,
                    reason: "the scene contains content the draw.io subset cannot represent".into(),
                });
                self.message = Some(StudioNotice::error(
                    "Draw.io export stopped because the committed scene has unsupported content; no bytes were published.",
                ));
            }
            (DrawioWorkKind::Export, DrawioWorkResult::Export(Err(error))) => {
                self.message = Some(StudioNotice::error(format!(
                    "Could not prepare draw.io export: {error}"
                )));
            }
            _ => {
                self.message = Some(StudioNotice::error(
                    "The draw.io worker returned an unexpected result.",
                ));
            }
        }
        ctx.request_repaint_of(egui::ViewportId::ROOT);
        if owner_viewport_id != egui::ViewportId::ROOT {
            ctx.request_repaint_of(owner_viewport_id);
        }
        true
    }

    fn drawio_fence_is_current(
        &self,
        fence: &DocumentActionFence,
        ctx: &egui::Context,
        owner_pass_current: bool,
    ) -> bool {
        (fence.owner_viewport_id == egui::ViewportId::ROOT
            || fence.owner_viewport_id == viewport_id())
            && ctx.viewport_id() == egui::ViewportId::ROOT
            && self.document_action_fence_is_current(fence, ctx, owner_pass_current)
    }

    fn drawio_scope_from_fence(fence: &DocumentActionFence) -> DrawioWorkflowScope {
        DrawioWorkflowScope {
            document: fence.document.clone(),
            revision: fence.revision,
            owner_generation: fence.owner_generation,
            owner_viewport_id: fence.owner_viewport_id,
        }
    }

    fn begin_drawio_import(
        &mut self,
        fence: &DocumentActionFence,
        ctx: &egui::Context,
        owner_pass_current: bool,
    ) {
        if !self.drawio_fence_is_current(fence, ctx, owner_pass_current) {
            self.message = Some(StudioNotice::error(
                "The active document changed before draw.io import; try again.",
            ));
            return;
        }
        if self.drawio_work.is_some()
            || self.drawio_review.is_some()
            || self.visual_asset_work.is_some()
            || self.completed_visual_export.is_some()
        {
            self.message = Some(StudioNotice::error(
                "Finish or cancel the current Diagram import/export before starting another.",
            ));
            return;
        }
        let Some(source_path) = self.pick_drawio_source() else {
            return;
        };
        if !self.drawio_fence_is_current(fence, ctx, owner_pass_current) {
            self.message = Some(StudioNotice::error(
                "The active document changed while choosing a draw.io source; nothing was imported.",
            ));
            return;
        }
        let scope = Self::drawio_scope_from_fence(fence);
        self.start_drawio_import(ctx, source_path, scope);
    }

    fn begin_drawio_export(
        &mut self,
        fence: &DocumentActionFence,
        ctx: &egui::Context,
        owner_pass_current: bool,
    ) {
        if !self.drawio_fence_is_current(fence, ctx, owner_pass_current)
            || self.documents.authoring_kind() != Some(crate::diagram::DiagramAuthoringKind::Visual)
            || self.documents.pending_transition().is_some()
        {
            self.message = Some(StudioNotice::error(
                "The visual document changed before draw.io export; try again.",
            ));
            return;
        }
        if self.visual_property_draft.is_some() {
            self.message = Some(StudioNotice::error(
                "Apply or cancel the visual property draft before exporting.",
            ));
            return;
        }
        if self.drawio_work.is_some()
            || self.drawio_review.is_some()
            || self.visual_asset_work.is_some()
            || self.completed_visual_export.is_some()
        {
            self.message = Some(StudioNotice::error(
                "Finish or cancel the current Diagram import/export before starting another.",
            ));
            return;
        }
        let Some(scene) = self.documents.visual_scene().cloned() else {
            self.message = Some(StudioNotice::error(
                "There is no active visual scene to export.",
            ));
            return;
        };
        self.start_drawio_export(ctx, scene, Self::drawio_scope_from_fence(fence));
    }

    fn publish_drawio_import_copy(
        &mut self,
        fence: &DocumentActionFence,
        ctx: &egui::Context,
        owner_pass_current: bool,
    ) {
        let (scope, source_path, candidate, has_copy) = match self.drawio_review.as_ref() {
            Some(DrawioWorkflowReview::Import {
                scope,
                source_path,
                candidate,
                copy_path,
            }) => (
                scope.clone(),
                source_path.clone(),
                candidate.clone(),
                copy_path.is_some(),
            ),
            _ => {
                self.message = Some(StudioNotice::error(
                    "There is no reviewed draw.io import to publish.",
                ));
                return;
            }
        };
        if has_copy
            || !scope.matches(self, fence.owner_viewport_id)
            || !self.drawio_fence_is_current(fence, ctx, owner_pass_current)
        {
            self.message = Some(StudioNotice::error(
                "The reviewed draw.io candidate is stale or already copied; review it again before publishing.",
            ));
            return;
        }
        let suggestion = source_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| format!("{stem}-imported.mld"))
            .unwrap_or_else(|| "drawio-import.mld".into());
        let Some(destination) = self.pick_copy_destination(&suggestion) else {
            return;
        };
        if !self.drawio_fence_is_current(fence, ctx, owner_pass_current)
            || !scope.matches(self, fence.owner_viewport_id)
        {
            self.message = Some(StudioNotice::error(
                "The active document changed while choosing a destination; no copy was written.",
            ));
            return;
        }
        if self.drawio_destination_aliases(&destination, Some(&source_path)) {
            return;
        }
        match publish_visual_document_copy(&destination, &candidate.document) {
            Ok(_) => {
                if let Some(DrawioWorkflowReview::Import { copy_path, .. }) =
                    self.drawio_review.as_mut()
                {
                    *copy_path = Some(destination.clone());
                }
                self.message = Some(StudioNotice::info(format!(
                    "Created a reviewed Visual copy at {}. The draw.io source and active document remain unchanged.",
                    destination.display()
                )));
            }
            Err(error) => {
                self.message = Some(StudioNotice::error(format!(
                    "Could not publish the Visual copy at {}: {error}",
                    destination.display()
                )));
            }
        }
    }

    fn open_drawio_import_copy(
        &mut self,
        fence: &DocumentActionFence,
        ctx: &egui::Context,
        owner_pass_current: bool,
    ) {
        let (scope, path) = match self.drawio_review.as_ref() {
            Some(DrawioWorkflowReview::Import {
                scope,
                copy_path: Some(path),
                ..
            }) => (scope.clone(), path.clone()),
            _ => {
                self.message = Some(StudioNotice::error(
                    "Create the reviewed .mld copy before opening it.",
                ));
                return;
            }
        };
        if !scope.matches(self, fence.owner_viewport_id)
            || !self.drawio_fence_is_current(fence, ctx, owner_pass_current)
        {
            self.message = Some(StudioNotice::error(
                "The reviewed Visual copy is stale; review the current document before opening it.",
            ));
            return;
        }
        match self.documents.request_open(&path) {
            Ok(outcome) => {
                self.finish_transition(outcome, false);
                if outcome == TransitionOutcome::Applied {
                    self.record_recent_path(&path, "opened");
                }
            }
            Err(error) => {
                self.show_document_error("open the reviewed Visual copy", Some(&path), error)
            }
        }
    }

    fn publish_drawio_export_review(
        &mut self,
        fence: &DocumentActionFence,
        ctx: &egui::Context,
        owner_pass_current: bool,
    ) {
        let (scope, result) = match self.drawio_review.as_ref() {
            Some(DrawioWorkflowReview::ExportReady { scope, result }) => {
                (scope.clone(), result.clone())
            }
            _ => {
                self.message = Some(StudioNotice::error(
                    "There is no complete draw.io export ready to publish.",
                ));
                return;
            }
        };
        if !result.report.issues.is_empty()
            || result.report.total_issues != 0
            || !scope.matches(self, fence.owner_viewport_id)
            || !self.drawio_fence_is_current(fence, ctx, owner_pass_current)
        {
            self.message = Some(StudioNotice::error(
                "The reviewed export is stale or reports unsupported content; no file was written.",
            ));
            return;
        }
        let suggested_name = self
            .documents
            .path()
            .and_then(Path::file_stem)
            .and_then(|stem| stem.to_str())
            .map(|stem| format!("{stem}.drawio"))
            .unwrap_or_else(|| "diagram.drawio".into());
        let Some(destination) = self.pick_drawio_destination(&suggested_name) else {
            return;
        };
        if !self.drawio_fence_is_current(fence, ctx, owner_pass_current)
            || !scope.matches(self, fence.owner_viewport_id)
        {
            self.message = Some(StudioNotice::error(
                "The visual document changed while choosing an export destination; no file was written.",
            ));
            return;
        }
        if self.drawio_destination_aliases(&destination, None) {
            return;
        }
        match publish_drawio_export(&destination, &result.bytes) {
            Ok(_) => {
                self.drawio_review = None;
                self.message = Some(StudioNotice::info(format!(
                    "Published draw.io file to {}.",
                    destination.display()
                )));
            }
            Err(error) => self.message = Some(StudioNotice::error(error.to_string())),
        }
    }

    fn drawio_destination_aliases(
        &mut self,
        destination: &Path,
        reviewed_source: Option<&Path>,
    ) -> bool {
        for protected in reviewed_source.into_iter().chain(self.documents.path()) {
            match same_export_destination(protected, destination) {
                Ok(true) => {
                    self.message = Some(StudioNotice::error(
                        "A draw.io destination cannot replace the active native document or reviewed source.",
                    ));
                    return true;
                }
                Ok(false) => {}
                Err(error) => {
                    self.message = Some(StudioNotice::error(format!(
                        "Could not verify that draw.io output is a distinct file: {error}"
                    )));
                    return true;
                }
            }
        }
        false
    }

    fn pick_export_destination(&mut self, format: DiagramExportFormat) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_file_picker_calls = self.test_file_picker_calls.saturating_add(1);
            if let Some(source) = self.test_picker_source_change.take() {
                let _ = self.documents.replace_source(&source);
                self.refresh_cache();
            }
            if let Some(label) = self.test_picker_visual_change.take() { let _ = self.documents.replace_visual_label(label); self.revalidate_visual_selection(); }
            if std::mem::take(&mut self.test_picker_host_change) {
                let next = if self.owner_host.presentation() == UtilityPresentation::Docked { UtilityPresentation::Popout } else { UtilityPresentation::Docked };
                self.owner_host.transition(next);
            }
            return self
                .test_next_file_picker_result
                .take()
                .unwrap_or(None)
                .map(|path| ensure_export_extension(path, format));
        }
        #[cfg(not(test))]
        {
            let picker = self.owned_file_dialog()?;
            let directory = self.picker_directory();
            let default_name = self
                .documents
                .path()
                .and_then(Path::file_stem)
                .and_then(|name| name.to_str())
                .unwrap_or("diagram");
            let file_name = format!("{default_name}.{}", format.extension());
            let mut picker = picker
                .add_filter(export_filter_name(format), &[format.extension()])
                .set_file_name(&file_name);
            if directory.is_dir() {
                picker = picker.set_directory(directory);
            }
            picker
                .save_file()
                .map(|path| ensure_export_extension(path, format))
        }
    }

    fn picker_directory(&self) -> &Path {
        self.documents
            .path()
            .and_then(Path::parent)
            .filter(|directory| directory.is_dir())
            .unwrap_or(self.default_directory.as_path())
    }

    fn finish_transition(&mut self, outcome: TransitionOutcome, closes: bool) -> bool {
        match outcome {
            TransitionOutcome::Applied => {
                self.message = None;
                if closes {
                    self.accept_close();
                    true
                } else {
                    self.reset_document_presentation();
                    self.open = true;
                    self.focus_source = true;
                    false
                }
            }
            TransitionOutcome::DecisionRequired => {
                self.close_pending = false;
                self.message = None;
                false
            }
            TransitionOutcome::Cancelled => {
                self.message = Some(StudioNotice::info("Document action cancelled."));
                false
            }
        }
    }

    fn show_document_error(
        &mut self,
        action: &str,
        path: Option<&Path>,
        error: DiagramDocumentError,
    ) {
        self.message = Some(StudioNotice::error(match path {
            Some(path) => format!("Could not {action} {}: {error}", path.display()),
            None => format!("Could not {action}: {error}"),
        }));
        self.open = true;
        self.close_pending = false;
    }

    fn show_current_document_error(&mut self, action: &str, error: DiagramDocumentError) {
        let path = self.documents.path().map(Path::to_path_buf);
        self.show_document_error(action, path.as_deref(), error);
    }

    fn pick_native_document(&mut self, save: bool) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_file_picker_calls = self.test_file_picker_calls.saturating_add(1);
            return self
                .test_next_file_picker_result
                .take()
                .unwrap_or(None)
                .map(|path| {
                    if save {
                        ensure_native_extension(path)
                    } else {
                        path
                    }
                });
        }
        #[cfg(not(test))]
        {
            let picker = self.owned_file_dialog()?;
            let current_directory = self
                .documents
                .path()
                .and_then(Path::parent)
                .filter(|directory| directory.is_dir());
            let directory = current_directory.unwrap_or(self.default_directory.as_path());
            let mut picker =
                picker.add_filter("Diagram Studio document", &[NATIVE_DIAGRAM_EXTENSION]);
            if directory.is_dir() {
                picker = picker.set_directory(directory);
            }
            if save {
                let file_name = self
                    .documents
                    .path()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    .unwrap_or("diagram.mld");
                picker
                    .set_file_name(file_name)
                    .save_file()
                    .map(ensure_native_extension)
            } else {
                picker.pick_file()
            }
        }
    }

    fn pick_copy_destination(&mut self, _suggested_name: &str) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_file_picker_calls = self.test_file_picker_calls.saturating_add(1);
            if let Some(source) = self.test_picker_source_change.take() {
                let _ = self.documents.replace_source(&source);
                self.refresh_cache();
            }
            return self
                .test_next_file_picker_result
                .take()
                .unwrap_or(None)
                .map(ensure_native_extension);
        }
        #[cfg(not(test))]
        {
            let picker = self.owned_file_dialog()?;
            let mut picker = picker
                .add_filter(
                    "Diagram Studio document (*.mld)",
                    &[NATIVE_DIAGRAM_EXTENSION],
                )
                .set_file_name(_suggested_name);
            let directory = self.picker_directory();
            if directory.is_dir() {
                picker = picker.set_directory(directory);
            }
            picker.save_file().map(ensure_native_extension)
        }
    }

    fn document_action_fence_is_current(
        &self,
        fence: &DocumentActionFence,
        ctx: &egui::Context,
        owner_pass_current: bool,
    ) -> bool {
        let current_revision = self
            .documents
            .session()
            .map(DiagramSession::revision)
            .or_else(|| self.documents.visual_revision())
            .unwrap_or_default();
        let owner_origin_matches = match fence.owner_pass {
            Some(pass) => pass.viewport_id == fence.owner_viewport_id && pass.was_retained(ctx),
            None => fence.owner_viewport_id == egui::ViewportId::ROOT,
        };
        fence.host == self.owner_host.identity()
            && fence.document == self.render_document
            && fence.revision == current_revision
            && fence.owner_generation == self.owner_generation
            && owner_origin_matches
            && fence.root_pass.was_retained(ctx)
            && owner_pass_current
    }

    fn ensure_recent_index_loaded(&mut self) {
        if self.recent_index_loaded {
            return;
        }
        self.recent_index_loaded = true;
        self.recent_index = match self.recent_index_path.as_deref() {
            Some(path) => match RecentDiagramIndex::load(path) {
                Ok(index) => Some(index),
                Err(error) => {
                    self.recent_index_error = Some(error.to_string());
                    None
                }
            },
            None => Some(RecentDiagramIndex::default()),
        };
    }

    fn record_recent_path(&mut self, path: &Path, operation: &str) {
        self.ensure_recent_index_loaded();
        if let Some(error) = self.recent_index_error.as_deref() {
            self.message = Some(StudioNotice::info(format!(
                "Diagram {operation} succeeded, but the existing recent list was left unchanged: {error}"
            )));
            return;
        }
        let Some(index) = self.recent_index.as_mut() else {
            return;
        };
        if let Err(error) = index.record(path) {
            self.recent_index_status = Some(error.to_string());
            self.message = Some(StudioNotice::info(format!(
                "Diagram {operation} succeeded, but it could not be added to recents: {error}"
            )));
            return;
        }
        if let Some(index_path) = self.recent_index_path.as_deref()
            && let Err(error) = index.save(index_path)
        {
            self.recent_index_status = Some(error.to_string());
            self.message = Some(StudioNotice::info(format!(
                "Diagram {operation} succeeded, but recents could not be saved: {error}"
            )));
        } else {
            self.recent_index_status = None;
        }
    }

    fn test_begin_frame(&mut self) {
        #[cfg(test)]
        {
            self.test_source_editor_rect = None;
            self.test_source_editor_enabled = false;
            self.test_canvas_rect = None;
            self.test_canvas_id = None;
            self.test_action_rects.clear();
            self.test_action_focus_ids.clear();
            self.test_discard_button_id = None;
            self.test_body_rendered = false;
            self.test_action_enabled.clear();
            self.test_generic_preview_drawn = false;
            self.test_preview_status_text = None;
            self.test_generic_canvas_placeholder = None;
            self.test_native_inspector_visible = false;
            self.test_viewer_callback_id = None;
            self.test_viewer_callback_class = None;
            self.test_viewer_body_rendered = false;
            self.test_viewer_canvas_rect = None;
            self.test_viewer_canvas_id = None;
            self.test_viewer_render_key = None;
            self.test_viewer_visual_projection_items = 0;
            self.test_viewer_visual_items_painted = 0;
            self.test_viewer_visual_items_omitted = 0;
            self.test_visual_items_painted = 0;
            self.test_visual_items_omitted = 0;
            self.test_visual_canvas_rect = None;
            self.test_visual_canvas_id = None;
            self.test_viewer_discard_requested = false;
        }
    }

    fn refresh_cache(&mut self) {
        let Some(session) = self.documents.session() else {
            self.cache = None;
            self.selection = None;
            self.selected_nodes.clear();
            self.property_draft = None;
            self.drag = None;
            return;
        };
        let revision = session.revision();
        if self
            .cache
            .as_ref()
            .is_some_and(|cache| cache.revision == revision)
        {
            return;
        }
        let status = session.status();
        let preview_graph = session.preview_graph();
        #[cfg(test)]
        if preview_graph.is_some() {
            self.test_scene_build_count = self.test_scene_build_count.saturating_add(1);
        }
        let (scene, scene_error) = preview_graph.map_or((None, None), |graph| {
            match build_scene(graph, SceneMetrics::default()) {
                Ok(scene) => (Some(scene), None),
                Err(error) => (None, Some(error.to_string())),
            }
        });
        self.cache = Some(RevisionCache {
            revision,
            status,
            scene,
            scene_error,
        });
        self.revalidate_selection();
        self.drag = None;
    }

    fn pick_visual_asset_file(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_file_picker_calls = self.test_file_picker_calls.saturating_add(1);
            return self.test_next_file_picker_result.take().unwrap_or(None);
        }
        #[cfg(not(test))]
        {
            let picker = self.owned_file_dialog()?;
            let directory = self.picker_directory();
            let mut picker = picker.add_filter("PNG or JPEG image", &["png", "jpg", "jpeg"]);
            if directory.is_dir() {
                picker = picker.set_directory(directory);
            }
            picker.pick_file()
        }
    }

    fn start_visual_asset_import(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        mut fence: VisualAssetImportFence,
    ) {
        if self.visual_asset_work.is_some()
            || self.completed_visual_asset_import.is_some()
            || self.completed_visual_export.is_some()
            || self.drawio_work.is_some()
        {
            self.message = Some(StudioNotice::error(
                "Wait for the current image operation to finish before importing another image.",
            ));
            return;
        }
        if !self.visual_intent_is_current(&fence.document, fence.revision)
            || fence.owner_generation != self.owner_generation
            || !fence.start_pass_accepted
            || fence.start_pass.viewport_id != fence.owner_viewport_id
            || !fence.start_pass.was_retained(ctx)
        {
            self.reject_stale_visual_intent();
            return;
        }
        fence.start_pass_accepted = true;
        let revision = fence.revision;
        let owner_viewport_id = fence.owner_viewport_id;
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let wake_context = ctx.clone();
        let renderer_max_side = ctx.input(|input| input.max_texture_side);
        let (sender, receiver) = sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("diagram-visual-asset-import".into())
            .spawn(move || {
                let result = if worker_cancelled.load(Ordering::Acquire) {
                    Err("image import was cancelled".to_owned())
                } else {
                    crate::diagram::prepare_visual_asset_file(&path, renderer_max_side)
                };
                if !worker_cancelled.load(Ordering::Acquire) {
                    let _ = sender.send(VisualAssetWorkResult::Import(result));
                }
                wake_context.request_repaint_of(owner_viewport_id);
            });
        if let Err(error) = worker {
            self.message = Some(StudioNotice::error(format!(
                "Could not start image import: {error}"
            )));
            return;
        }
        self.visual_asset_work = Some(PendingVisualAssetWork {
            document: fence.document.clone(),
            revision,
            owner_generation: self.owner_generation,
            owner_viewport_id,
            kind: VisualAssetWorkKind::Import(fence),
            cancelled,
            receiver,
            hydration_ids: Vec::new(),
            retired: false,
        });
        self.message = Some(StudioNotice::info(
            "Importing image; the document will update after the accepted owner pass.",
        ));
    }

    fn process_visual_asset_picker_request(
        &mut self,
        ctx: &egui::Context,
        effects: &PendingRootEffects,
        owner_pass_current: bool,
    ) {
        let Some(mut fence) = effects.visual_asset_picker_requested.clone() else {
            return;
        };
        let pass_matches = if effects.owner_pass.is_some() {
            effects.owner_pass == Some(fence.start_pass)
        } else {
            effects.root_pass == fence.start_pass
        };
        if !owner_pass_current
            || !effects.visual_structural_editing_allowed
            || !effects.owner_pass_accepted
            || !pass_matches
            || !fence.start_pass.was_retained(ctx)
            || effects.close_dialog
            || effects.close_viewport
            || effects.os_close_requested
            || effects.document_intent.is_some()
            || effects.mutation.is_some()
            || fence.owner_generation != self.owner_generation
            || fence.owner_viewport_id != effects.owner_viewport_id
            || !self.visual_intent_is_current(&fence.document, fence.revision)
        {
            return;
        }

        let Some(path) = self.pick_visual_asset_file() else {
            return;
        };
        // Native file pickers can outlive the initiating frame. Recheck the
        // document fence immediately after they return before starting IO.
        if effects.close_dialog
            || effects.close_viewport
            || effects.document_intent.is_some()
            || fence.owner_generation != self.owner_generation
            || fence.owner_viewport_id != effects.owner_viewport_id
            || !self.visual_intent_is_current(&fence.document, fence.revision)
        {
            return;
        }
        fence.start_pass_accepted = true;
        self.start_visual_asset_import(ctx, path, fence);
    }

    fn start_visual_asset_hydration(
        &mut self,
        ctx: &egui::Context,
        scene: &VisualScene,
        revision: u64,
    ) {
        if self.visual_asset_work.is_some()
            || self.completed_visual_asset_import.is_some()
            || self.completed_visual_export.is_some()
            || self.drawio_work.is_some()
        {
            return;
        }
        let assets = scene
            .assets
            .iter()
            .filter(|asset| {
                !self.visual_asset_textures.contains_key(asset.id())
                    && !self.visual_asset_pixels.contains_key(asset.id())
                    && !self.visual_asset_errors.contains_key(asset.id())
            })
            .cloned()
            .collect::<Vec<_>>();
        if assets.is_empty() {
            return;
        }
        let asset_ids = assets
            .iter()
            .map(|asset| asset.id().clone())
            .collect::<Vec<_>>();
        let document = self.render_document.clone();
        let owner_generation = self.owner_generation;
        let owner_viewport_id = ctx.viewport_id();
        let renderer_max_side = ctx.input(|input| input.max_texture_side);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let wake_context = ctx.clone();
        let (sender, receiver) = sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("diagram-visual-asset-decode".into())
            .spawn(move || {
                let mut decoded = Vec::with_capacity(assets.len());
                for asset in &assets {
                    if worker_cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    decoded.push(
                        crate::diagram::decode_visual_asset_payload(asset, renderer_max_side)
                            .map_err(|error| (asset.id().clone(), error)),
                    );
                }
                if !worker_cancelled.load(Ordering::Acquire) {
                    let _ = sender.send(VisualAssetWorkResult::Hydrate(decoded));
                }
                wake_context.request_repaint_of(owner_viewport_id);
            });
        if let Err(error) = worker {
            for id in asset_ids {
                self.visual_asset_errors
                    .insert(id, format!("could not start image decoder: {error}"));
            }
            return;
        }
        self.visual_asset_work = Some(PendingVisualAssetWork {
            document,
            revision,
            owner_generation,
            owner_viewport_id,
            kind: VisualAssetWorkKind::Hydrate,
            cancelled,
            receiver,
            hydration_ids: asset_ids,
            retired: false,
        });
    }

    fn retire_visual_asset_work(&mut self) {
        if let Some(work) = self.visual_asset_work.as_mut() {
            work.retired = true;
            work.cancelled.store(true, Ordering::Release);
        }
        self.completed_visual_asset_import = None;
        self.completed_visual_export = None;
    }

    fn clear_visual_asset_state(&mut self) {
        self.retire_visual_asset_work();
        self.visual_asset_textures.clear();
        self.visual_asset_pixels.clear();
        self.visual_asset_errors.clear();
        self.visual_asset_document = None;
        self.visual_asset_revision = None;
        self.visual_asset_owner_generation = self.owner_generation;
    }

    fn sync_visual_asset_scope(
        &mut self,
        scene: &VisualScene,
        revision: u64,
        pending: bool,
        speculative_close: bool,
    ) {
        let identity = self.render_document.clone();
        if self.visual_asset_document.as_ref() != Some(&identity) {
            self.retire_visual_asset_work();
            self.visual_asset_textures.clear();
            self.visual_asset_pixels.clear();
            self.visual_asset_errors.clear();
            self.visual_asset_document = Some(identity.clone());
            self.visual_asset_revision = Some(revision);
            self.visual_asset_owner_generation = self.owner_generation;
        } else {
            if self.visual_asset_owner_generation != self.owner_generation {
                self.retire_visual_asset_work();
                self.visual_asset_owner_generation = self.owner_generation;
            }
            if self.visual_asset_revision != Some(revision) {
                if self
                    .visual_asset_work
                    .as_ref()
                    .is_some_and(|work| work.document != identity || work.revision != revision)
                {
                    self.retire_visual_asset_work();
                }
                if self
                    .completed_visual_asset_import
                    .as_ref()
                    .is_some_and(|import| {
                        import.fence.document != identity || import.fence.revision != revision
                    })
                {
                    self.completed_visual_asset_import = None;
                }
                if self.completed_visual_export.as_ref().is_some_and(|export| {
                    export.fence.document != identity || export.revision != revision
                }) {
                    self.completed_visual_export = None;
                }
                self.visual_asset_revision = Some(revision);
            }
        }
        if pending {
            self.retire_visual_asset_work();
        } else if speculative_close {
            self.retire_visual_asset_work_preserving_export();
        }
        let active_assets = scene
            .assets
            .iter()
            .map(|asset| asset.id().clone())
            .collect::<std::collections::HashSet<_>>();
        self.visual_asset_textures
            .retain(|id, _| active_assets.contains(id));
        self.visual_asset_pixels
            .retain(|id, _| active_assets.contains(id));
        self.visual_asset_errors
            .retain(|id, _| active_assets.contains(id));
        if self
            .completed_visual_asset_import
            .as_ref()
            .is_some_and(|import| {
                import.fence.document != identity
                    || import.fence.revision != revision
                    || import.fence.owner_generation != self.owner_generation
            })
        {
            self.completed_visual_asset_import = None;
        }
        if self.completed_visual_export.as_ref().is_some_and(|export| {
            export.fence.document != identity
                || export.revision != revision
                || export.fence.owner_generation != self.owner_generation
        }) {
            self.completed_visual_export = None;
        }
    }

    fn retire_visual_asset_work_preserving_export(&mut self) {
        if let Some(work) = self.visual_asset_work.as_mut()
            && !matches!(&work.kind, VisualAssetWorkKind::Export)
        {
            work.retired = true;
            work.cancelled.store(true, Ordering::Release);
        }
        self.completed_visual_asset_import = None;
    }

    fn cancel_visual_export(&mut self) {
        let mut running = false;
        if let Some(work) = self.visual_asset_work.as_mut()
            && matches!(&work.kind, VisualAssetWorkKind::Export)
            && !work.retired
        {
            work.retired = true;
            work.cancelled.store(true, Ordering::Release);
            running = true;
        }
        let staged = self.completed_visual_export.take().is_some();
        self.message = Some(StudioNotice::info(if running {
            "Visual export cancelled; its worker slot remains occupied until it settles."
        } else if staged {
            "Staged visual export cancelled."
        } else {
            "There is no visual export to cancel."
        }));
    }

    fn poll_visual_asset_work(&mut self) {
        let Some(work) = self.visual_asset_work.as_ref() else {
            return;
        };
        let work_kind = match &work.kind {
            VisualAssetWorkKind::Import(_) => VisualAssetWorkKindTag::Import,
            VisualAssetWorkKind::Hydrate => VisualAssetWorkKindTag::Hydrate,
            VisualAssetWorkKind::Export => VisualAssetWorkKindTag::Export,
        };
        let was_retired = work.retired;
        let hydration_ids = work.hydration_ids.clone();
        let result = match work.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Disconnected) => None,
            Err(TryRecvError::Empty) => return,
        };
        if result.is_none() {
            self.visual_asset_work = None;
            if !was_retired {
                match work_kind {
                    VisualAssetWorkKindTag::Import => {
                        self.message = Some(StudioNotice::error(
                            "Image import worker stopped before returning a result.",
                        ));
                    }
                    VisualAssetWorkKindTag::Export => {
                        self.message = Some(StudioNotice::error(
                            "Visual export worker stopped before returning a result.",
                        ));
                    }
                    VisualAssetWorkKindTag::Hydrate => {
                        for id in hydration_ids {
                            self.visual_asset_errors.insert(
                                id,
                                "image decoder stopped before returning this resource".into(),
                            );
                        }
                    }
                }
            }
            return;
        }
        let work = self
            .visual_asset_work
            .take()
            .expect("work was checked above");
        if work.retired
            || work.document != self.render_document
            || Some(work.revision) != self.documents.visual_revision()
            || work.owner_generation != self.owner_generation
            || self.documents.pending_transition().is_some()
        {
            return;
        }
        match (work.kind, result.expect("available worker result")) {
            (VisualAssetWorkKind::Import(fence), VisualAssetWorkResult::Import(Ok(prepared))) => {
                if fence.start_pass_accepted {
                    self.completed_visual_asset_import =
                        Some(CompletedVisualAssetImport { fence, prepared });
                    self.message = Some(StudioNotice::info(
                        "Image is ready; accepting it in the current Diagram Studio pass.",
                    ));
                }
            }
            (VisualAssetWorkKind::Import(_), VisualAssetWorkResult::Import(Err(error))) => {
                self.message = Some(StudioNotice::error(format!(
                    "Could not import image: {error}"
                )));
            }
            (VisualAssetWorkKind::Hydrate, VisualAssetWorkResult::Hydrate(decoded)) => {
                for result in decoded {
                    match result {
                        Ok(asset) => {
                            self.visual_asset_errors.remove(&asset.id);
                            self.visual_asset_pixels.insert(asset.id.clone(), asset);
                        }
                        Err((id, error)) => {
                            self.visual_asset_errors.insert(id, error);
                        }
                    }
                }
            }
            (VisualAssetWorkKind::Export, VisualAssetWorkResult::Export(Ok(export))) => {
                if export.fence.document == work.document
                    && export.revision == work.revision
                    && export.fence.owner_generation == work.owner_generation
                    && export.fence.owner_viewport_id == work.owner_viewport_id
                {
                    self.completed_visual_export = Some(export);
                    self.message = Some(StudioNotice::info(
                        "Visual export is staged; it will publish after an accepted owner pass.",
                    ));
                } else {
                    self.message = Some(StudioNotice::error(
                        "Visual export worker returned a mismatched document result.",
                    ));
                }
            }
            (VisualAssetWorkKind::Export, VisualAssetWorkResult::Export(Err(error))) => {
                self.message = Some(StudioNotice::error(format!(
                    "Could not create visual export: {error}"
                )));
            }
            _ => {
                self.message = Some(StudioNotice::error(
                    "Visual background worker returned an unexpected result.",
                ));
            }
        }
    }

    fn upload_visual_asset_textures(&mut self, ctx: &egui::Context) {
        let Some(id) = self
            .visual_asset_pixels
            .keys()
            .filter(|id| !self.visual_asset_textures.contains_key(*id))
            .min()
            .cloned()
        else {
            return;
        };
        let max_texture_side = ctx.input(|input| input.max_texture_side);
        if let Some(pixels) = self.visual_asset_pixels.get(&id)
            && (pixels.width as usize > max_texture_side
                || pixels.height as usize > max_texture_side)
        {
            self.visual_asset_pixels.remove(&id);
            self.visual_asset_errors.insert(
                id,
                format!("image exceeds the renderer's {max_texture_side}-pixel texture side limit"),
            );
            return;
        }
        let Some(pixels) = self.visual_asset_pixels.remove(&id) else {
            return;
        };
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [pixels.width as usize, pixels.height as usize],
            &pixels.rgba,
        );
        let texture = ctx.load_texture(
            format!("diagram-visual-asset-{}", id.as_str()),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.visual_asset_textures.insert(
            id,
            VisualAssetTexture {
                texture,
                width: pixels.width,
                height: pixels.height,
            },
        );
        if !self.visual_asset_pixels.is_empty() {
            ctx.request_repaint_of(ctx.viewport_id());
        }
    }

    fn stage_completed_visual_asset_import(
        &mut self,
        ctx: &egui::Context,
        actions_allowed: bool,
        closing_requested: bool,
        pending: &mut Option<PendingMutation>,
        document_intent_pending: bool,
    ) {
        if !actions_allowed
            || closing_requested
            || pending.is_some()
            || document_intent_pending
            || self.documents.pending_transition().is_some()
        {
            return;
        }
        let Some(import) = self.completed_visual_asset_import.as_ref() else {
            return;
        };
        let fence = &import.fence;
        if ctx.viewport_id() != fence.owner_viewport_id
            || !fence.start_pass_accepted
            || fence.document != self.render_document
            || Some(fence.revision) != self.documents.visual_revision()
            || fence.owner_generation != self.owner_generation
        {
            self.completed_visual_asset_import = None;
            return;
        }
        let import = self
            .completed_visual_asset_import
            .take()
            .expect("completed image was checked above");
        let mut transform = next_visual_object_transform(self.documents.visual_scene());
        let (width, height) = import.prepared.asset.dimensions();
        let scale = (180.0 / f64::from(width.max(1)))
            .min(120.0 / f64::from(height.max(1)))
            .max(crate::diagram::MIN_VISUAL_DIMENSION / f64::from(width.min(height).max(1)));
        transform.width = f64::from(width) * scale;
        transform.height = f64::from(height) * scale;
        if transform.width < crate::diagram::MIN_VISUAL_DIMENSION
            || transform.height < crate::diagram::MIN_VISUAL_DIMENSION
        {
            self.message = Some(StudioNotice::error(
                "Image dimensions cannot be represented within the visual document bounds.",
            ));
            return;
        }
        *pending = Some(PendingMutation::VisualAssetImport {
            prepared: import.prepared,
            layer_id: DEFAULT_LAYER_ID,
            transform,
            style: VisualStyle::default(),
            fence: import.fence,
        });
    }

    fn visual_projection(
        &mut self,
    ) -> Option<(
        Arc<VisualScene>,
        Result<Arc<VisualSceneProjection>, VisualProjectionError>,
    )> {
        let revision = self.documents.visual_revision()?;
        if let Some(cache) = self.visual_projection_cache.as_ref()
            && cache.document == self.render_document
            && cache.revision == revision
        {
            return Some((cache.scene.clone(), cache.projection.clone()));
        }
        let scene = Arc::new(self.documents.visual_scene()?.clone());
        let projection = project_visual_scene(&scene).map(Arc::new);
        self.visual_projection_cache = Some(VisualProjectionCache {
            document: self.render_document.clone(),
            revision,
            scene: scene.clone(),
            projection: projection.clone(),
        });
        #[cfg(test)]
        {
            self.test_visual_projection_build_count =
                self.test_visual_projection_build_count.saturating_add(1);
        }
        Some((scene, projection))
    }

    fn revalidate_visual_selection(&mut self) {
        let Some(scene) = self.documents.visual_scene() else {
            self.visual_selection = None;
            self.visual_selected_objects.clear();
            self.visual_property_draft = None;
            self.visual_gesture = None;
            self.visual_canvas_press_authorized = None;
            return;
        };
        let previous_primary = self.visual_selection;
        self.visual_selected_objects
            .retain(|id| scene.objects.iter().any(|object| object.id == *id));
        if self
            .visual_selection
            .is_some_and(|id| !self.visual_selected_objects.contains(&id))
        {
            self.visual_selection = self.visual_selected_objects.last().copied();
        }
        if previous_primary != self.visual_selection {
            self.visual_property_draft = None;
            self.visual_membership_target = None;
        }
        if self.visual_membership_target.is_some_and(|target| {
            self.visual_selection
                .is_none_or(|child| !visual_frame_can_parent(scene, child, target))
        }) {
            self.visual_membership_target = None;
        }
        let revision = self.documents.visual_revision();
        if self.visual_property_draft.as_ref().is_some_and(|draft| {
            draft.document != self.render_document || Some(draft.basis_revision) != revision
        }) {
            self.visual_property_draft = None;
        }
        if self
            .visual_selected_preset
            .is_some_and(|id| !scene.presets.iter().any(|preset| preset.id == id))
        {
            self.visual_selected_preset = None;
        }
        if self.visual_gesture.as_ref().is_some_and(|gesture| {
            gesture.document != self.render_document
                || Some(gesture.revision) != self.documents.visual_revision()
        }) {
            self.visual_gesture = None;
            self.visual_canvas_press_authorized = None;
        }
    }

    fn set_visual_selection(&mut self, mut selection: VisualSelectionState) {
        if let Some(scene) = self.documents.visual_scene() {
            selection
                .objects
                .retain(|id| scene.objects.iter().any(|object| object.id == *id));
        } else {
            selection.objects.clear();
        }
        if selection
            .primary
            .is_none_or(|primary| !selection.objects.contains(&primary))
        {
            selection.primary = selection.objects.last().copied();
        }
        if self.visual_selection != selection.primary {
            self.visual_property_draft = None;
            self.visual_membership_target = None;
        }
        self.visual_selection = selection.primary;
        self.visual_selected_objects = selection.objects;
    }

    fn sync_generic_preview_after_accepted_pass(
        &mut self,
        ctx: &egui::Context,
        repaint_target: egui::ViewportId,
        debounce_source_edit: bool,
    ) {
        const RASTER_WIDTH: u32 = 1_024;
        const RASTER_HEIGHT: u32 = 768;

        let Some(session) = self.documents.session() else {
            if !matches!(&self.generic_preview, GenericPreviewStatus::Idle) {
                if let Some(renderer) = &self.renderer {
                    renderer.retire(&self.render_document);
                }
                self.generic_preview = GenericPreviewStatus::Idle;
            }
            self.generic_texture = None;
            return;
        };
        let revision = session.revision();
        let source = session.draft();
        if source.is_empty() || !is_generic_preview_candidate(source) {
            if !matches!(&self.generic_preview, GenericPreviewStatus::Idle) {
                if let Some(renderer) = &self.renderer {
                    renderer.retire(&self.render_document);
                }
                self.generic_preview = GenericPreviewStatus::Idle;
            }
            self.generic_texture = None;
            return;
        }

        let raster_size = RasterSize::new(RASTER_WIDTH, RASTER_HEIGHT)
            .expect("Studio preview raster bounds are valid");
        let key = RenderKey::new(self.render_document.clone(), revision, self.render_options);
        let request_matches_status = match &self.generic_preview {
            GenericPreviewStatus::Pending {
                key: current_key,
                raster_size: current_size,
            }
            | GenericPreviewStatus::Ready {
                key: current_key,
                raster_size: current_size,
                ..
            }
            | GenericPreviewStatus::Failed {
                key: current_key,
                raster_size: current_size,
                ..
            } => current_key == &key && current_size == &raster_size,
            GenericPreviewStatus::Idle => false,
        };
        if request_matches_status {
            return;
        }

        self.generic_preview = GenericPreviewStatus::Pending {
            key: key.clone(),
            raster_size,
        };
        let repaint_context = ctx.clone();
        let viewer_open = self.viewer_open;
        let wake = std::sync::Arc::new(move || {
            repaint_context.request_repaint_of(repaint_target);
            if viewer_open {
                repaint_context.request_repaint_of(viewer_viewport_id());
                repaint_context.request_repaint_of(egui::ViewportId::ROOT);
            }
        });
        let renderer = self.renderer.get_or_insert_with(RendererService::default);
        if let Err(error) = renderer.submit(
            key.clone(),
            std::sync::Arc::from(source),
            raster_size,
            debounce_source_edit,
            wake,
        ) {
            self.generic_preview = GenericPreviewStatus::Failed {
                key,
                raster_size,
                error,
            };
        }
    }

    fn publish_generic_preview_completion(&mut self) -> bool {
        if let Some(renderer) = &self.renderer
            && let Some(completion) = renderer.take_completion()
        {
            return self.generic_preview.apply(completion);
        }
        false
    }

    fn current_generic_key(&self) -> Option<RenderKey> {
        let session = self.documents.session()?;
        generic_preview_family(session.draft())?;
        Some(RenderKey::new(
            self.render_document.clone(),
            session.revision(),
            self.render_options,
        ))
    }

    fn ensure_generic_texture(&mut self, ctx: &egui::Context) {
        let Some(key) = self.current_generic_key() else {
            return;
        };
        let Some(output) = generic_preview_output(&self.generic_preview, &key).cloned() else {
            return;
        };
        if self
            .generic_texture
            .as_ref()
            .is_some_and(|texture| texture.key == key)
        {
            return;
        }

        let raster = &output.raster;
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [raster.width as usize, raster.height as usize],
            &raster.rgba,
        );
        let texture = ctx.load_texture(
            "diagram-studio-generic-preview",
            color_image,
            egui::TextureOptions::LINEAR,
        );
        if self.generic_texture.is_none() {
            self.camera.fit_next = true;
        }
        self.generic_texture = Some(GenericPreviewTexture {
            key,
            texture,
            natural_width: output.natural_width,
            natural_height: output.natural_height,
            content_uv: raster_content_uv(raster),
        });
    }

    fn revalidate_selection(&mut self) {
        let Some(graph) = self
            .documents
            .session()
            .and_then(DiagramSession::preview_graph)
        else {
            self.selection = None;
            self.selected_nodes.clear();
            return;
        };
        self.selected_nodes
            .retain(|id| graph.nodes.iter().any(|node| &node.id == id));
        let selection_exists = self
            .selection
            .as_ref()
            .is_some_and(|selected| match selected {
                DiagramElement::Node(id) => graph.nodes.iter().any(|node| &node.id == id),
                DiagramElement::Edge(id) => graph.edges.iter().any(|edge| edge.id == *id),
                DiagramElement::Group(id) => graph.groups.iter().any(|group| &group.id == id),
            });
        if !selection_exists {
            self.selection = None;
            self.selected_nodes.clear();
            self.property_draft = None;
        }
    }

    fn apply_pending(&mut self, pending: PendingMutation) -> bool {
        match pending {
            PendingMutation::ReplaceSourceWithCursor {
                source,
                expected_document,
                expected_revision,
                ..
            } => {
                let source_is_current = self.render_document == expected_document
                    && self.documents.authoring_kind()
                        == Some(crate::diagram::DiagramAuthoringKind::Mermaid)
                    && self
                        .documents
                        .session()
                        .is_some_and(|session| session.revision() == expected_revision)
                    && self.documents.pending_transition().is_none();
                if !source_is_current {
                    self.message = Some(StudioNotice::error(
                        "The Mermaid source changed while this suggestion was open. Choose a suggestion again.",
                    ));
                    return false;
                }
                self.apply_mermaid_pending(PendingMutation::ReplaceSource(source))
            }
            PendingMutation::ReplaceVisualLabel {
                label,
                expected_document,
                expected_revision,
            } => {
                if !self.visual_intent_is_current(&expected_document, expected_revision) {
                    return self.reject_stale_visual_intent();
                }
                match self.documents.replace_visual_label(label) {
                    Ok(changed) => {
                        self.message = None;
                        if changed {
                            self.visual_property_draft = None;
                            self.revalidate_visual_selection();
                        }
                        true
                    }
                    Err(error) => {
                        self.message = Some(StudioNotice::error(error.to_string()));
                        false
                    }
                }
            }
            PendingMutation::VisualTransaction {
                operations,
                expected_document,
                expected_revision,
                select_after,
            } => {
                if !self.visual_intent_is_current(&expected_document, expected_revision) {
                    return self.reject_stale_visual_intent();
                }
                match self.documents.apply_visual_transaction(&operations) {
                    Ok(outcome) => {
                        self.message = None;
                        self.visual_property_draft = None;
                        if outcome.changed {
                            if let Some(selection) = select_after {
                                self.visual_selection = Some(selection);
                                if !self.visual_selected_objects.contains(&selection) {
                                    self.visual_selected_objects.clear();
                                    self.visual_selected_objects.insert(selection);
                                }
                            } else if let Some(selection) = outcome.created_objects.last().copied()
                            {
                                self.visual_selected_objects.clear();
                                self.visual_selected_objects.insert(selection);
                                self.visual_selection = Some(selection);
                            }
                            if let Some(preset) = outcome.created_presets.last().copied() {
                                self.visual_selected_preset = Some(preset);
                            }
                            self.revalidate_visual_selection();
                        }
                        true
                    }
                    Err(error) => {
                        self.message = Some(StudioNotice::error(error.to_string()));
                        false
                    }
                }
            }
            PendingMutation::VisualAssetImport {
                prepared,
                layer_id,
                transform,
                style,
                fence,
            } => {
                if !self.visual_intent_is_current(&fence.document, fence.revision)
                    || fence.owner_generation != self.owner_generation
                    || !fence.start_pass_accepted
                {
                    return self.reject_stale_visual_intent();
                }
                let asset_id = prepared.asset.id().clone();
                let needs_pixels = !self.visual_asset_textures.contains_key(&asset_id)
                    && !self.visual_asset_pixels.contains_key(&asset_id);
                let decoded = needs_pixels.then(|| DecodedVisualAsset {
                    id: asset_id.clone(),
                    width: prepared.asset.dimensions().0,
                    height: prepared.asset.dimensions().1,
                    rgba: prepared.rgba.clone(),
                });
                match self.documents.apply_visual_transaction(&[
                    VisualOperation::AddImageReference {
                        asset: prepared.asset,
                        layer_id,
                        transform,
                        style,
                    },
                ]) {
                    Ok(outcome) => {
                        self.message = None;
                        self.visual_property_draft = None;
                        if outcome.changed {
                            self.visual_asset_errors.remove(&asset_id);
                            if let Some(decoded) = decoded {
                                self.visual_asset_pixels.insert(asset_id, decoded);
                            }
                            if let Some(id) = outcome.created_objects.last().copied() {
                                self.visual_selected_objects.clear();
                                self.visual_selected_objects.insert(id);
                                self.visual_selection = Some(id);
                            }
                            self.revalidate_visual_selection();
                        }
                        true
                    }
                    Err(error) => {
                        self.message = Some(StudioNotice::error(error.to_string()));
                        false
                    }
                }
            }
            PendingMutation::VisualLayoutFailure {
                message,
                expected_document,
                expected_revision,
            } => {
                if !self.visual_intent_is_current(&expected_document, expected_revision) {
                    return self.reject_stale_visual_intent();
                }
                self.message = Some(StudioNotice::error(message));
                false
            }
            PendingMutation::VisualGestureTransaction {
                operations,
                expected_document,
                expected_revision,
                owner_generation,
                owner_viewport_id,
                start_pass,
                start_pass_accepted,
            } => {
                if !self.visual_intent_is_current(&expected_document, expected_revision)
                    || owner_generation != self.owner_generation
                    || owner_viewport_id != start_pass.viewport_id
                    || !start_pass_accepted
                {
                    return self.reject_stale_visual_intent();
                }
                match self.documents.apply_visual_transaction(&operations) {
                    Ok(outcome) => {
                        self.message = None;
                        self.visual_property_draft = None;
                        if outcome.changed {
                            self.revalidate_visual_selection();
                        }
                        true
                    }
                    Err(error) => {
                        self.message = Some(StudioNotice::error(error.to_string()));
                        false
                    }
                }
            }
            PendingMutation::VisualHistory {
                redo,
                expected_document,
                expected_revision,
            } => {
                if !self.visual_intent_is_current(&expected_document, expected_revision) {
                    return self.reject_stale_visual_intent();
                }
                let result = if redo {
                    self.documents.redo()
                } else {
                    self.documents.undo()
                };
                match result {
                    Ok(changed) => {
                        self.message = None;
                        if changed {
                            self.visual_property_draft = None;
                            self.revalidate_visual_selection();
                        }
                        true
                    }
                    Err(error) => {
                        self.message = Some(StudioNotice::error(error.to_string()));
                        false
                    }
                }
            }
            pending => self.apply_mermaid_pending(pending),
        }
    }

    fn apply_mermaid_pending(&mut self, pending: PendingMutation) -> bool {
        if matches!(&pending, PendingMutation::Undo | PendingMutation::Redo)
            && self.documents.authoring_kind()
                != Some(crate::diagram::DiagramAuthoringKind::Mermaid)
        {
            self.message = Some(StudioNotice::error(
                "Mermaid history is no longer active for this document.",
            ));
            return false;
        }
        let result = match pending {
            PendingMutation::ReplaceSource(source) => self
                .documents
                .replace_source(source)
                .map(|changed| (changed, None)),
            PendingMutation::ReplaceVisualLabel { .. }
            | PendingMutation::VisualTransaction { .. }
            | PendingMutation::VisualAssetImport { .. }
            | PendingMutation::VisualLayoutFailure { .. }
            | PendingMutation::VisualGestureTransaction { .. }
            | PendingMutation::VisualHistory { .. } => {
                unreachable!("visual mutation dispatched above")
            }
            PendingMutation::ReplaceSourceWithCursor { .. } => {
                unreachable!("source assistance mutation dispatched above")
            }
            PendingMutation::Transaction {
                operations,
                expected_revision,
                select_after,
            } => {
                if self
                    .documents
                    .session()
                    .is_none_or(|session| session.revision() != expected_revision)
                {
                    self.message = Some(StudioNotice::error(
                        "The diagram changed while these properties were open. Reload them before applying.",
                    ));
                    return false;
                }
                match self.documents.apply_transaction(&operations) {
                    Ok(changed) => Ok((changed, select_after)),
                    Err(error) => Err(error),
                }
            }
            PendingMutation::Undo => self.documents.undo().map(|changed| (changed, None)),
            PendingMutation::Redo => self.documents.redo().map(|changed| (changed, None)),
        };
        match result {
            Ok((changed, select_after)) => {
                self.message = None;
                if changed {
                    if let Some(selection) = select_after {
                        self.set_selection(selection, false);
                    }
                    self.refresh_cache();
                }
                true
            }
            Err(error) => {
                self.message = Some(StudioNotice::error(error.to_string()));
                false
            }
        }
    }

    fn visual_intent_is_current(
        &self,
        expected_document: &RenderDocumentIdentity,
        expected_revision: u64,
    ) -> bool {
        self.render_document == *expected_document
            && self.documents.visual_revision() == Some(expected_revision)
            && self.documents.pending_transition().is_none()
    }

    fn reject_stale_visual_intent(&mut self) -> bool {
        self.message = Some(StudioNotice::error(
            "The visual document changed while these properties were open. Reload them before applying.",
        ));
        false
    }

    #[cfg(test)]
    pub(crate) fn test_replace_source(&mut self, source: &str) {
        let _ = self.documents.replace_source(source);
        self.refresh_cache();
    }

    #[cfg(test)]
    pub(crate) fn test_stage_markdown_source_handoff(
        &mut self,
        origin: DiagramSourceCopyOrigin,
        source: impl Into<String>,
    ) {
        self.pending_markdown_source = Some(DiagramMarkdownSource {
            origin,
            source: source.into(),
        });
    }

    fn reset_document_presentation(&mut self) {
        self.clear_visual_asset_state();
        self.retire_drawio_work();
        if let Some(renderer) = &self.renderer {
            renderer.retire(&self.render_document);
        }
        self.render_document = RenderDocumentIdentity::fresh();
        self.generic_preview = GenericPreviewStatus::Idle;
        self.generic_texture = None;
        self.cache = None;
        self.visual_projection_cache = None;
        self.selection = None;
        self.selected_nodes.clear();
        self.property_draft = None;
        if self.documents.visual_scene().is_some() {
            self.visual_selection = Some(crate::diagram::TITLE_OBJECT_ID);
            self.visual_selected_objects = [crate::diagram::TITLE_OBJECT_ID].into_iter().collect();
        } else {
            self.visual_selection = None;
            self.visual_selected_objects.clear();
        }
        self.visual_selected_preset = None;
        self.visual_membership_target = None;
        self.visual_property_draft = None;
        self.visual_gesture = None;
        self.visual_canvas_press_authorized = None;
        self.drag = None;
        self.camera = Camera::default();
        self.viewer_camera = Camera::default();
        self.visual_viewer_fit_follow = true;
        self.visual_viewer_fit_size = None;
        self.visual_viewer_minimap_drag = None;
        self.visual_editor_minimap_drag = None;
        self.message = None;
        self.deferred_canvas_focus = None;
        self.deferred_source_focus = None;
        self.deferred_viewer_canvas_focus = None;
        self.deferred_menu_focus = None;
        self.active_menu_scope = None;
        self.refresh_cache();
    }

    #[cfg(test)]
    pub(crate) fn test_request_child_discard(&mut self) {
        self.test_request_child_discard = true;
    }

    #[cfg(test)]
    fn test_request_viewer_child_discard(&mut self) {
        self.test_request_viewer_discard = true;
    }

    #[cfg(test)]
    fn test_canvas_has_focus(&self) -> bool {
        self.test_canvas_id.is_some_and(|id| {
            super::render::immediate_viewport_test_focused_id()
                .is_some_and(|(viewport, focused)| viewport == viewport_id() && focused == Some(id))
        })
    }

    #[cfg(test)]
    fn test_viewer_canvas_has_focus(&self) -> bool {
        self.test_viewer_canvas_id
            .is_some_and(|id| self.test_viewer_memory_focused_id == Some(id))
    }

    #[cfg(test)]
    fn test_viewer_generic_preview_drawn(&self) -> bool {
        self.test_viewer_generic_preview_drawn
    }

    #[cfg(test)]
    fn test_last_viewer_callback(
        &self,
    ) -> (Option<egui::ViewportId>, Option<egui::ViewportClass>, bool) {
        (
            self.test_viewer_callback_id,
            self.test_viewer_callback_class,
            self.test_viewer_body_rendered,
        )
    }

    #[cfg(test)]
    pub(crate) fn test_stage_add_node(&mut self) {
        let id = NodeId::new("FromUi").expect("test node ID is valid");
        self.test_pending_mutation = Some(PendingMutation::Transaction {
            operations: vec![GraphOperation::AddNode {
                id: id.clone(),
                label: "From UI".into(),
                shape: NodeShape::Rectangle,
                position: Position::new(900.0, 900.0).expect("test position is valid"),
                group: None,
            }],
            expected_revision: self
                .documents
                .session()
                .expect("test dialog has an active session")
                .revision(),
            select_after: Some(DiagramElement::Node(id)),
        })
    }

    #[cfg(test)]
    pub(crate) fn test_session(&self) -> &DiagramSession {
        self.documents
            .session()
            .expect("test dialog has an active session")
    }

    #[cfg(test)]
    pub(crate) fn test_pending_transition(&self) -> Option<&PendingDiagramTransition> {
        self.documents.pending_transition()
    }

    #[cfg(test)]
    pub(crate) fn test_has_active_document(&self) -> bool {
        self.documents.has_active_document()
    }

    #[cfg(test)]
    fn test_stage_document_intent(&mut self, intent: DocumentIntent) {
        self.test_pending_document_intent = Some(intent);
    }

    #[cfg(test)]
    pub(crate) fn test_stage_pending_decision(&mut self, decision: TransitionDecision) {
        self.test_pending_document_intent = Some(DocumentIntent::ResolvePending(decision));
    }

    #[cfg(test)]
    pub(crate) fn test_set_picker_visual_change(&mut self, label: impl Into<String>) { self.test_picker_visual_change = Some(label.into()); }

    #[cfg(test)]
    pub(crate) fn test_set_next_file_picker_result(&mut self, result: Option<PathBuf>) {
        self.test_next_file_picker_result = Some(result);
    }

    #[cfg(test)]
    pub(crate) fn test_file_picker_calls(&self) -> usize {
        self.test_file_picker_calls
    }

    #[cfg(test)]
    pub(crate) fn test_action_enabled(&self, name: &str) -> Option<bool> {
        self.test_action_enabled
            .iter()
            .find(|(action, _)| action == name)
            .map(|(_, enabled)| *enabled)
    }

    #[cfg(test)]
    pub(crate) fn test_generic_preview_drawn(&self) -> bool {
        self.test_generic_preview_drawn
    }

    #[cfg(test)]
    pub(crate) fn test_preview_status_text(&self) -> Option<&str> {
        self.test_preview_status_text.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn test_generic_canvas_placeholder(&self) -> Option<&str> {
        self.test_generic_canvas_placeholder.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn test_native_inspector_visible(&self) -> bool {
        self.test_native_inspector_visible
    }

    #[cfg(test)]
    pub(crate) fn test_set_picker_source_change(&mut self, source: impl Into<String>) {
        self.test_picker_source_change = Some(source.into());
    }

    #[cfg(test)]
    pub(crate) fn test_set_picker_visual_label_change(&mut self, label: impl Into<String>) {
        self.test_picker_visual_label_change = Some(label.into());
    }

    #[cfg(test)]
    pub(crate) fn test_action_rect(&self, name: &str) -> Option<(egui::Rect, egui::Rect)> {
        self.test_action_rects
            .iter()
            .find(|(action, _, _)| action == name)
            .map(|(_, rect, clip)| (*rect, *clip))
    }

    #[cfg(test)]
    pub(crate) fn test_last_callback(
        &self,
    ) -> (
        Option<egui::ViewportId>,
        Option<egui::ViewportClass>,
        bool,
        bool,
    ) {
        (
            self.test_callback_id,
            self.test_callback_class,
            self.test_body_rendered,
            self.test_application_input_allowed,
        )
    }

    #[cfg(test)]
    pub(crate) fn test_source_editor_has_focus(&self, _ctx: &egui::Context) -> bool {
        self.test_source_editor_id.is_some_and(|id| {
            super::render::immediate_viewport_test_focused_id()
                .is_some_and(|(viewport, focused)| viewport == viewport_id() && focused == Some(id))
        })
    }

    #[cfg(test)]
    pub(crate) fn test_source_editor_enabled(&self) -> bool {
        self.test_source_editor_enabled
    }

    #[cfg(test)]
    fn test_action_has_focus(&self, name: &str) -> bool {
        let Some((_, expected)) = self
            .test_action_focus_ids
            .iter()
            .find(|(action, _)| action == name)
        else {
            return false;
        };
        super::render::immediate_viewport_test_focused_id().is_some_and(|(viewport, focused)| {
            viewport == viewport_id() && focused == Some(*expected)
        })
    }

    #[cfg(test)]
    pub(crate) fn test_source_focus_pending(&self) -> bool {
        self.focus_source
    }

    #[cfg(test)]
    pub(crate) fn test_move_child_focus_to(&mut self, id: egui::Id) {
        self.deferred_canvas_focus = Some((viewport_id(), id));
    }

    fn set_selection(&mut self, element: DiagramElement, extend: bool) {
        match &element {
            DiagramElement::Node(id) if extend => {
                if !self.selected_nodes.insert(id.clone()) {
                    self.selected_nodes.remove(id);
                }
                self.selection = if self.selected_nodes.contains(id) {
                    Some(element)
                } else {
                    self.selected_nodes
                        .last()
                        .cloned()
                        .map(DiagramElement::Node)
                };
            }
            DiagramElement::Node(id) => {
                self.selected_nodes.clear();
                self.selected_nodes.insert(id.clone());
                self.selection = Some(element);
            }
            _ => {
                self.selected_nodes.clear();
                self.selection = Some(element);
            }
        }
        self.property_draft = None;
    }

    fn draw_body(
        &mut self,
        ui: &mut egui::Ui,
        actions_allowed: bool,
        keyboard_actions_allowed: bool,
        closing_requested: bool,
        pending: &mut Option<PendingMutation>,
        document_intent: &mut Option<DocumentIntent>,
        canvas_focus_requested: &mut Option<egui::Id>,
        source_focus_requested: &mut Option<egui::Id>,
        source_focus_allowed: bool,
    ) -> DrawBodyEffects {
        if self.documents.authoring_kind() == Some(crate::diagram::DiagramAuthoringKind::Visual) {
            return self.draw_visual_body(
                ui,
                actions_allowed,
                closing_requested,
                pending,
                document_intent,
                canvas_focus_requested,
            );
        }
        self.ensure_generic_texture(ui.ctx());
        self.refresh_cache();
        let focused_at_start = ui.memory(|memory| memory.focused());
        let previous_menu_scope = self.active_menu_scope.as_ref().cloned();
        let mut draw_effects = DrawBodyEffects::default();
        let mut active_menu_scope: Option<MenuFocusScope> = None;
        #[cfg(test)]
        if !closing_requested && pending.is_none() {
            *pending = self.test_pending_mutation.take();
        }
        #[cfg(test)]
        if !closing_requested && document_intent.is_none() {
            *document_intent = self.test_pending_document_intent.take();
        }
        #[cfg(test)]
        {
            self.test_body_rendered = true;
        }
        let drawio_controls_available = self.drawio_work.is_none()
            && self.visual_asset_work.is_none()
            && self.completed_visual_asset_import.is_none()
            && self.drawio_review.is_none();
        let drawio_review_current = self
            .drawio_review
            .as_ref()
            .is_some_and(|review| review.scope().matches(self, ui.ctx().viewport_id()));
        let Self {
            documents,
            view_mode,
            source_preview_split_ratio,
            inspector_canvas_split_ratio,
            export_scale,
            viewer_open,
            cache,
            camera,
            render_document,
            render_options,
            generic_preview,
            generic_texture,
            selection,
            selected_nodes,
            property_draft,
            drag,
            message,
            focus_source,
            recent_index,
            recent_index_loaded,
            recent_index_error,
            recent_index_status,
            recent_search,
            recent_panel_open,
            drawio_work,
            drawio_review,
            #[cfg(test)]
            test_source_editor_id,
            #[cfg(test)]
            test_source_editor_rect,
            #[cfg(test)]
            test_source_editor_enabled,
            #[cfg(test)]
            test_canvas_rect,
            #[cfg(test)]
            test_canvas_id,
            #[cfg(test)]
            test_action_rects,
            #[cfg(test)]
            test_action_enabled,
            #[cfg(test)]
            test_generic_preview_drawn,
            #[cfg(test)]
            test_preview_status_text,
            #[cfg(test)]
            test_generic_canvas_placeholder,
            #[cfg(test)]
            test_native_inspector_visible,
            #[cfg(test)]
            test_action_focus_ids,
            #[cfg(test)]
            test_discard_button_id,
            ..
        } = self;
        let session = documents
            .session()
            .expect("open Diagram Studio has an active document");
        let pending_transition = documents.pending_transition().cloned();
        let document_path = documents.path().map(Path::to_path_buf);
        let document_dirty = documents.is_dirty();
        let can_undo = documents.can_undo();
        let can_redo = documents.can_redo();
        let can_convert_visual = session.current_graph().is_some_and(|graph| {
            graph.groups.is_empty() && graph.nodes.iter().all(|node| node.group.is_none())
        });
        let popup_open_at_start = egui::Popup::is_any_open(ui.ctx());
        let document_actions_allowed = actions_allowed
            && !closing_requested
            && pending_transition.is_none()
            && !popup_open_at_start;
        let menu_actions_allowed =
            actions_allowed && !closing_requested && pending_transition.is_none();
        let revision = session.revision();
        let cache = cache.as_ref().expect("cache refreshed before drawing");
        let status = &cache.status;
        let graph = session.preview_graph();
        let generic_family = generic_preview_family(session.draft());
        let native_editable = status.preview == PreviewState::Current && graph.is_some();
        let generic_key = generic_family
            .map(|_| RenderKey::new(render_document.clone(), revision, *render_options));
        let generic_output = generic_key
            .as_ref()
            .and_then(|key| generic_preview_output(generic_preview, key));
        let image_export_available = native_editable || generic_output.is_some();
        let generic_texture = generic_texture.as_ref().filter(|texture| {
            texture.key.document == *render_document && texture.key.options == *render_options
        });
        let showing_stale_generic =
            generic_texture.is_some_and(|texture| texture.key.accepted_revision != revision);
        let generic_status = generic_key.as_ref().and_then(|key| {
            generic_preview_status_text(generic_preview, key, showing_stale_generic)
        });
        let generic_canvas_placeholder = generic_key
            .as_ref()
            .map_or("Preparing Mermaid preview…", |key| {
                generic_preview_canvas_placeholder(generic_preview, key)
            });
        let base_editing_allowed =
            actions_allowed && !closing_requested && pending_transition.is_none();
        if base_editing_allowed
            && keyboard_actions_allowed
            && !popup_open_at_start
            && pending.is_none()
            && let Some(action) = take_history_shortcut(ui)
        {
            *pending = Some(action);
        }

        egui::ScrollArea::vertical()
            .id_salt("diagram-studio-body-scroll")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading("Diagram Studio");
                    let viewer_button = ui.add_enabled(
                        document_actions_allowed,
                        egui::Button::new(if *viewer_open {
                            "Focus viewer"
                        } else {
                            "Open viewer"
                        }),
                    );
                    #[cfg(test)]
                    test_action_rects.push((
                        "open_viewer".into(),
                        viewer_button.rect,
                        ui.clip_rect(),
                    ));
                    #[cfg(test)]
                    test_action_enabled.push((
                        "open_viewer".into(),
                        viewer_button.enabled() && ui.is_visible() && !ui.is_sizing_pass(),
                    ));
                    if viewer_button.clicked() {
                        draw_effects.open_viewer_requested = true;
                    }
                    ui.separator();
                    for (_name, label, intent, enabled) in [
                        (
                            "document_new",
                            "New",
                            DocumentIntent::New,
                            document_actions_allowed,
                        ),
                        (
                            "document_new_visual",
                            "New Visual",
                            DocumentIntent::NewVisual,
                            document_actions_allowed,
                        ),
                        (
                            "document_open",
                            "Open…",
                            DocumentIntent::Open,
                            document_actions_allowed,
                        ),
                        (
                            "document_save",
                            "Save",
                            DocumentIntent::Save,
                            document_actions_allowed,
                        ),
                        (
                            "document_save_as",
                            "Save As…",
                            DocumentIntent::SaveAs,
                            document_actions_allowed,
                        ),
                        (
                            "document_close",
                            "Close",
                            DocumentIntent::Close,
                            document_actions_allowed,
                        ),
                    ] {
                        let response = ui.add_enabled(enabled, egui::Button::new(label));
                        #[cfg(test)]
                        test_action_rects.push((_name.into(), response.rect, ui.clip_rect()));
                        if response.clicked() && document_intent.is_none() {
                            *document_intent = Some(intent);
                        }
                    }
                    if let Some(origin) = documents.source_copy_origin().cloned() {
                        let response = ui.add_enabled(
                            document_actions_allowed,
                            egui::Button::new("Use source in Markdown"),
                        );
                        #[cfg(test)]
                        test_action_rects.push((
                            "use_source_in_markdown".into(),
                            response.rect,
                            ui.clip_rect(),
                        ));
                        if response.clicked() && document_intent.is_none() {
                            *document_intent = Some(DocumentIntent::UseSourceInMarkdown(origin));
                        }
                    }
                    let convert = ui.add_enabled(
                        document_actions_allowed && can_convert_visual,
                        egui::Button::new("Copy as Visual…"),
                    );
                    #[cfg(test)]
                    {
                        test_action_rects.push((
                            "convert_visual_copy".into(),
                            convert.rect,
                            ui.clip_rect(),
                        ));
                        test_action_enabled.push((
                            "convert_visual_copy".into(),
                            document_actions_allowed && can_convert_visual,
                        ));
                    }
                    if convert.clicked() && document_intent.is_none() {
                        *document_intent = Some(DocumentIntent::ConvertMermaidCopy);
                    }
                    if !can_convert_visual {
                        ui.weak("Visual copy requires a valid native flowchart without groups.");
                    }
                    let mut import_item_ids = Vec::new();
                    let mut import_sizing_pass = false;
                    let _import_menu = ui.menu_button("Import", |menu_ui| {
                        import_sizing_pass = menu_ui.is_sizing_pass();
                        let response = menu_ui.add_enabled(
                            menu_actions_allowed,
                            egui::Button::new("Mermaid source…"),
                        );
                        if response.enabled() {
                            import_item_ids.push(response.id);
                        }
                        #[cfg(test)]
                        test_action_rects.push((
                            "import_mermaid".into(),
                            response.rect,
                            menu_ui.clip_rect(),
                        ));
                        #[cfg(test)]
                        test_action_focus_ids.push(("import_mermaid".into(), response.id));
                        if response.clicked() && document_intent.is_none() {
                            *document_intent = Some(DocumentIntent::ImportMermaid);
                            menu_ui.close();
                        }
                        let drawio = menu_ui.add_enabled(
                            menu_actions_allowed && drawio_controls_available,
                            egui::Button::new("Draw.io diagram…"),
                        );
                        #[cfg(test)]
                        test_action_rects.push((
                            "drawio_import".into(),
                            drawio.rect,
                            menu_ui.clip_rect(),
                        ));
                        if drawio.enabled() {
                            import_item_ids.push(drawio.id);
                        }
                        if drawio.clicked() && document_intent.is_none() {
                            *document_intent = Some(DocumentIntent::DrawioImport);
                            menu_ui.close();
                        }
                    });
                    #[cfg(test)]
                    test_action_rects.push((
                        "menu_import".into(),
                        _import_menu.response.rect,
                        ui.clip_rect(),
                    ));
                    #[cfg(test)]
                    test_action_focus_ids.push(("menu_import".into(), _import_menu.response.id));
                    let import_popup_id = egui::Popup::default_response_id(&_import_menu.response);
                    if egui::Popup::is_id_open(ui.ctx(), import_popup_id) {
                        active_menu_scope = Some(MenuFocusScope {
                            popup_id: import_popup_id,
                            trigger_id: _import_menu.response.id,
                            item_ids: import_item_ids,
                            sizing_pass: import_sizing_pass,
                        });
                    }

                    let mut template_item_ids = Vec::new();
                    let mut templates_sizing_pass = false;
                    let _templates_menu = ui.menu_button("Templates", |menu_ui| {
                        templates_sizing_pass = menu_ui.is_sizing_pass();
                        for template in DiagramTemplate::ALL {
                            let response = menu_ui.add_enabled(
                                menu_actions_allowed,
                                egui::Button::new(template.name()),
                            );
                            if response.enabled() {
                                template_item_ids.push(response.id);
                            }
                            #[cfg(test)]
                            test_action_rects.push((
                                format!("template_{}", template.name().to_lowercase()),
                                response.rect,
                                menu_ui.clip_rect(),
                            ));
                            #[cfg(test)]
                            test_action_focus_ids.push((
                                format!("template_{}", template.name().to_lowercase()),
                                response.id,
                            ));
                            if response.clicked() && document_intent.is_none() {
                                *document_intent = Some(DocumentIntent::ApplyTemplate(template));
                                menu_ui.close();
                            }
                        }
                    });
                    #[cfg(test)]
                    test_action_rects.push((
                        "menu_templates".into(),
                        _templates_menu.response.rect,
                        ui.clip_rect(),
                    ));
                    #[cfg(test)]
                    test_action_focus_ids
                        .push(("menu_templates".into(), _templates_menu.response.id));
                    let templates_popup_id =
                        egui::Popup::default_response_id(&_templates_menu.response);
                    if egui::Popup::is_id_open(ui.ctx(), templates_popup_id) {
                        active_menu_scope = Some(MenuFocusScope {
                            popup_id: templates_popup_id,
                            trigger_id: _templates_menu.response.id,
                            item_ids: template_item_ids,
                            sizing_pass: templates_sizing_pass,
                        });
                    }

                    let mut export_item_ids = Vec::new();
                    let mut export_sizing_pass = false;
                    let _export_menu = ui.menu_button("Export", |menu_ui| {
                        export_sizing_pass = menu_ui.is_sizing_pass();
                        for (format, label, _name) in [
                            (DiagramExportFormat::Mermaid, "Mermaid source…", "mermaid"),
                            (DiagramExportFormat::Svg, "SVG image…", "svg"),
                            (DiagramExportFormat::Png, "PNG image…", "png"),
                        ] {
                            let enabled = menu_actions_allowed
                                && (format == DiagramExportFormat::Mermaid
                                    || image_export_available);
                            let response = menu_ui.add_enabled(enabled, egui::Button::new(label));
                            #[cfg(test)]
                            test_action_enabled.push((format!("export_{_name}"), enabled));
                            if response.enabled() {
                                export_item_ids.push(response.id);
                            }
                            #[cfg(test)]
                            test_action_rects.push((
                                format!("export_{_name}"),
                                response.rect,
                                menu_ui.clip_rect(),
                            ));
                            #[cfg(test)]
                            test_action_focus_ids.push((format!("export_{_name}"), response.id));
                            if response.clicked() && document_intent.is_none() {
                                *document_intent =
                                    Some(DocumentIntent::Export(format, *export_scale));
                                menu_ui.close();
                            }
                        }
                        menu_ui.separator();
                        menu_ui.label("Image scale");
                        menu_ui.horizontal_wrapped(|menu_ui| {
                            for (scale, label, _name) in ExportScale::ALL {
                                let response = menu_ui.add_enabled(
                                    menu_actions_allowed,
                                    egui::Button::selectable(*export_scale == scale, label),
                                );
                                if response.enabled() {
                                    export_item_ids.push(response.id);
                                }
                                #[cfg(test)]
                                test_action_rects.push((
                                    format!("export_scale_{_name}"),
                                    response.rect,
                                    menu_ui.clip_rect(),
                                ));
                                #[cfg(test)]
                                test_action_focus_ids
                                    .push((format!("export_scale_{_name}"), response.id));
                                if response.clicked() && menu_actions_allowed {
                                    draw_effects.export_scale_requested = Some(scale);
                                    menu_ui.close();
                                }
                            }
                        });
                    });
                    #[cfg(test)]
                    test_action_rects.push((
                        "menu_export".into(),
                        _export_menu.response.rect,
                        ui.clip_rect(),
                    ));
                    #[cfg(test)]
                    test_action_focus_ids.push(("menu_export".into(), _export_menu.response.id));
                    let export_popup_id = egui::Popup::default_response_id(&_export_menu.response);
                    if egui::Popup::is_id_open(ui.ctx(), export_popup_id) {
                        active_menu_scope = Some(MenuFocusScope {
                            popup_id: export_popup_id,
                            trigger_id: _export_menu.response.id,
                            item_ids: export_item_ids,
                            sizing_pass: export_sizing_pass,
                        });
                    }
                    let background_actions_allowed =
                        base_editing_allowed && active_menu_scope.is_none();
                    if let Some(path) = document_path.as_deref() {
                        ui.weak(
                            path.file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("diagram.mld"),
                        );
                    } else {
                        ui.weak("Untitled");
                    }
                    if document_dirty {
                        ui.weak("Modified");
                    }
                    ui.separator();
                    let source_mode = ui.selectable_label(view_mode.is_source(), "Source");
                    #[cfg(test)]
                    test_action_rects.push((
                        "view_source".into(),
                        source_mode.rect,
                        ui.clip_rect(),
                    ));
                    if background_actions_allowed && source_mode.clicked() {
                        *view_mode = view_mode.hide_preview();
                    }
                    for (mode, label) in [
                        (PreviewMode::Visual, "Visual"),
                        (PreviewMode::Split, "Split"),
                    ] {
                        let selected = !view_mode.is_source()
                            && *view_mode == ViewMode::from_preview_mode(mode);
                        let response = ui.selectable_label(selected, label);
                        #[cfg(test)]
                        test_action_rects.push((
                            format!("view_{}", label.to_lowercase()),
                            response.rect,
                            ui.clip_rect(),
                        ));
                        if background_actions_allowed && response.clicked() {
                            *view_mode = ViewMode::from_preview_mode(mode);
                        }
                    }
                    if view_mode.is_source() {
                        let response = ui.add_enabled(
                            background_actions_allowed,
                            egui::Button::new("Show preview"),
                        );
                        #[cfg(test)]
                        test_action_rects.push((
                            "show_preview".into(),
                            response.rect,
                            ui.clip_rect(),
                        ));
                        if response.clicked() {
                            *view_mode = view_mode.shows_preview();
                        }
                    }
                    ui.separator();
                    let undo = ui.add_enabled(
                        background_actions_allowed && can_undo,
                        egui::Button::new("Undo"),
                    );
                    if undo.clicked() && pending.is_none() {
                        *pending = Some(PendingMutation::Undo);
                    }
                    let redo = ui.add_enabled(
                        background_actions_allowed && can_redo,
                        egui::Button::new("Redo"),
                    );
                    if redo.clicked() && pending.is_none() {
                        *pending = Some(PendingMutation::Redo);
                    }
                    let zoom_out =
                        ui.add_enabled(background_actions_allowed, egui::Button::new("−"));
                    #[cfg(test)]
                    test_action_rects.push(("zoom_out".into(), zoom_out.rect, ui.clip_rect()));
                    if zoom_out.clicked() {
                        camera.zoom = (camera.zoom / CAMERA_KEYBOARD_ZOOM_FACTOR)
                            .clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
                        camera.fit_next = false;
                    }
                    let _zoom_percentage = ui.label(format!("{:.0}%", camera.zoom * 100.0));
                    #[cfg(test)]
                    test_action_rects.push((
                        "zoom_percentage".into(),
                        _zoom_percentage.rect,
                        ui.clip_rect(),
                    ));
                    let zoom_in =
                        ui.add_enabled(background_actions_allowed, egui::Button::new("+"));
                    #[cfg(test)]
                    test_action_rects.push(("zoom_in".into(), zoom_in.rect, ui.clip_rect()));
                    if zoom_in.clicked() {
                        camera.zoom = (camera.zoom * CAMERA_KEYBOARD_ZOOM_FACTOR)
                            .clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
                        camera.fit_next = false;
                    }
                    let fit = ui.add_enabled(background_actions_allowed, egui::Button::new("Fit"));
                    #[cfg(test)]
                    test_action_rects.push(("fit".into(), fit.rect, ui.clip_rect()));
                    if fit.clicked() {
                        camera.fit_next = true;
                    }
                    let zoom_100 =
                        ui.add_enabled(background_actions_allowed, egui::Button::new("100%"));
                    #[cfg(test)]
                    test_action_rects.push(("zoom_100".into(), zoom_100.rect, ui.clip_rect()));
                    if zoom_100.clicked() {
                        camera.zoom = 1.0;
                        camera.fit_next = false;
                    }
                    let reset_view =
                        ui.add_enabled(background_actions_allowed, egui::Button::new("Reset view"));
                    #[cfg(test)]
                    test_action_rects.push(("reset_view".into(), reset_view.rect, ui.clip_rect()));
                    if reset_view.clicked() {
                        *camera = Camera::default();
                        camera.fit_next = false;
                    }
                });

                draw_drawio_workflow_panel(
                    ui,
                    drawio_work.as_ref(),
                    drawio_review.as_ref(),
                    drawio_review_current,
                    document_actions_allowed,
                    document_intent,
                    #[cfg(test)]
                    test_action_rects,
                );

                draw_effects.recent_panel = draw_recent_documents(
                    ui,
                    recent_index.as_ref(),
                    *recent_index_loaded,
                    recent_index_error.as_deref(),
                    recent_index_status.as_deref(),
                    recent_search,
                    *recent_panel_open,
                    document_actions_allowed,
                    document_intent,
                    #[cfg(test)]
                    test_action_rects,
                );

                let menu_open = active_menu_scope.is_some();
                let editing_allowed = base_editing_allowed && !menu_open;
                let source_edit_allowed =
                    actions_allowed && pending_transition.is_none() && !menu_open;
                let visual_mutations_allowed = editing_allowed && native_editable;
                let decision_actions_allowed = actions_allowed && !closing_requested && !menu_open;

                ui.horizontal_wrapped(|ui| {
                    let add_node = ui.add_enabled(
                        visual_mutations_allowed
                            && graph.is_some_and(|graph| graph.nodes.len() < MAX_DIAGRAM_NODES),
                        egui::Button::new("Add node"),
                    );
                    #[cfg(test)]
                    test_action_enabled.push(("add_node".into(), add_node.enabled()));
                    #[cfg(test)]
                    test_action_rects.push(("add_node".into(), add_node.rect, ui.clip_rect()));
                    if add_node.clicked()
                        && pending.is_none()
                        && let Some(graph) = graph
                    {
                        match next_node(graph, camera.center) {
                            Ok((id, position)) => {
                                *pending = Some(PendingMutation::Transaction {
                                    operations: vec![GraphOperation::AddNode {
                                        id: id.clone(),
                                        label: "New node".into(),
                                        shape: NodeShape::Rectangle,
                                        position,
                                        group: None,
                                    }],
                                    expected_revision: revision,
                                    select_after: Some(DiagramElement::Node(id)),
                                });
                            }
                            Err(error) => *message = Some(StudioNotice::error(error)),
                        }
                    }
                    if ui
                        .add_enabled(
                            visual_mutations_allowed
                                && graph.is_some_and(|graph| {
                                    graph.nodes.len() >= 2
                                        && graph.edges.len() < crate::diagram::MAX_DIAGRAM_EDGES
                                }),
                            egui::Button::new("Add edge"),
                        )
                        .clicked()
                        && pending.is_none()
                        && let Some(graph) = graph
                    {
                        let from = graph.nodes.first().map(|node| node.id.clone());
                        let to = graph.nodes.get(1).map(|node| node.id.clone());
                        if let (Some(from), Some(to)) = (from, to) {
                            *pending = Some(PendingMutation::Transaction {
                                operations: vec![GraphOperation::AddEdge {
                                    from,
                                    to,
                                    label: None,
                                }],
                                expected_revision: revision,
                                select_after: None,
                            });
                        }
                    }
                    if ui
                        .add_enabled(
                            visual_mutations_allowed
                                && graph
                                    .is_some_and(|graph| graph.groups.len() < MAX_DIAGRAM_GROUPS),
                            egui::Button::new("Add group"),
                        )
                        .clicked()
                        && pending.is_none()
                        && let Some(graph) = graph
                    {
                        match next_group_id(graph) {
                            Ok(id) => {
                                *pending = Some(PendingMutation::Transaction {
                                    operations: vec![GraphOperation::AddGroup {
                                        id: id.clone(),
                                        label: "New group".into(),
                                    }],
                                    expected_revision: revision,
                                    select_after: Some(DiagramElement::Group(id)),
                                });
                            }
                            Err(error) => *message = Some(StudioNotice::error(error)),
                        }
                    }
                    let can_remove = visual_mutations_allowed
                        && (!selected_nodes.is_empty()
                            || selection.as_ref().is_some_and(|selected| {
                                !matches!(selected, DiagramElement::Node(_))
                            }));
                    if ui
                        .add_enabled(can_remove, egui::Button::new("Remove selection"))
                        .clicked()
                        && pending.is_none()
                        && graph.is_some()
                    {
                        let operations = removal_operations(selection.as_ref(), selected_nodes);
                        if !operations.is_empty() {
                            *pending = Some(PendingMutation::Transaction {
                                operations,
                                expected_revision: revision,
                                select_after: None,
                            });
                        }
                    }
                    let auto_layout =
                        ui.add_enabled(visual_mutations_allowed, egui::Button::new("Auto-layout"));
                    #[cfg(test)]
                    test_action_enabled.push(("auto_layout".into(), auto_layout.enabled()));
                    #[cfg(test)]
                    test_action_rects.push((
                        "auto_layout".into(),
                        auto_layout.rect,
                        ui.clip_rect(),
                    ));
                    if auto_layout.clicked() && document_intent.is_none() {
                        *document_intent = Some(DocumentIntent::AutoLayout);
                    }
                });
                let _direction_row = ui.horizontal(|ui| {
                    ui.label("Direction");
                    direction_control(
                        ui,
                        graph,
                        native_editable,
                        editing_allowed,
                        revision,
                        pending,
                    );
                });
                #[cfg(test)]
                test_action_rects.push((
                    "direction".into(),
                    _direction_row.response.rect,
                    ui.clip_rect(),
                ));

                draw_effects.menu_focus_request = menu_focus_request_for_ui(
                    ui, active_menu_scope.as_ref(), previous_menu_scope.as_ref(), actions_allowed,
                    closing_requested, focused_at_start,
                );
                draw_effects.active_menu_scope = active_menu_scope;

                if let Some(transition) = pending_transition.as_ref() {
                    ui.group(|ui| {
                        let description = match transition {
                            PendingDiagramTransition::New => {
                                "starting a new Mermaid document".to_owned()
                            }
                            PendingDiagramTransition::NewVisual => {
                                "starting a new visual document".to_owned()
                            }
                            PendingDiagramTransition::Open(path) => {
                                format!("opening {}", path.display())
                            }
                            PendingDiagramTransition::ImportMermaid(file) => {
                                format!("importing {}", file.path().display())
                            }
                            PendingDiagramTransition::CopySource(_) => {
                                "copying Mermaid source from Markdown".to_owned()
                            }
                            PendingDiagramTransition::ApplyTemplate(template) => {
                                format!("using the {} template", template.name())
                            }
                            PendingDiagramTransition::ApplyVisualTemplate(template) => format!("using the {} visual template", template.name()),
                            PendingDiagramTransition::Close => "closing this diagram".to_owned(),
                        };
                        ui.label(format!("Save changes before {description}?"));
                        ui.horizontal_wrapped(|ui| {
                            for (_name, label, decision) in [
                                ("decision_save", "Save", TransitionDecision::Save),
                                ("decision_discard", "Discard", TransitionDecision::Discard),
                                ("decision_cancel", "Cancel", TransitionDecision::Cancel),
                            ] {
                                let response = ui.add_enabled(
                                    decision_actions_allowed,
                                    egui::Button::new(label),
                                );
                                #[cfg(test)]
                                test_action_rects.push((
                                    _name.into(),
                                    response.rect,
                                    ui.clip_rect(),
                                ));
                                #[cfg(test)]
                                if _name == "decision_discard" {
                                    *test_discard_button_id = Some(response.id);
                                }
                                if response.clicked() && document_intent.is_none() {
                                    *document_intent =
                                        Some(DocumentIntent::ResolvePending(decision));
                                }
                            }
                        });
                    });
                }

                if generic_family.is_some() {
                    #[cfg(test)]
                    {
                        *test_preview_status_text =
                            generic_status.as_ref().map(|(_, text)| text.clone());
                    }
                    if let Some((color, text)) = generic_status.as_ref() {
                        ui.colored_label(*color, text);
                    }
                } else if status.preview == PreviewState::Stale {
                    let diagnostic = status
                        .diagnostic
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "The source has changed.".into());
                    ui.colored_label(
                        egui::Color32::from_rgb(190, 104, 18),
                        format!("STALE PREVIEW — {diagnostic} Visual changes are disabled."),
                    );
                } else if status.preview == PreviewState::NoPreview {
                    let diagnostic = status
                        .diagnostic
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "No valid preview is available.".into());
                    ui.colored_label(egui::Color32::from_rgb(185, 65, 65), diagnostic);
                }
                if let Some(error) = message.as_ref() {
                    match error {
                        StudioNotice::Info(_) => {
                            ui.label(error.text());
                        }
                        StudioNotice::Error(_) => {
                            ui.colored_label(egui::Color32::from_rgb(185, 65, 65), error.text());
                        }
                    }
                }
                if let Some(error) = cache.scene_error.as_ref() {
                    ui.colored_label(egui::Color32::from_rgb(185, 65, 65), error);
                }
                ui.separator();

                match *view_mode {
                    ViewMode::Source { .. } => source_editor(
                        ui,
                        session,
                        render_document,
                        source_edit_allowed,
                        pending,
                        *focus_source && source_focus_allowed,
                        source_focus_requested,
                        #[cfg(test)]
                        test_source_editor_id,
                        #[cfg(test)]
                        test_source_editor_rect,
                        #[cfg(test)]
                        test_source_editor_enabled,
                        #[cfg(test)]
                        test_action_rects,
                        #[cfg(test)]
                        test_action_focus_ids,
                    ),
                    ViewMode::Visual => preview_workspace(
                        ui,
                        generic_family.is_some(),
                        generic_texture,
                        generic_canvas_placeholder,
                        graph,
                        cache.scene.as_ref(),
                        native_editable,
                        revision,
                        camera,
                        inspector_canvas_split_ratio,
                        selection,
                        selected_nodes,
                        property_draft,
                        drag,
                        editing_allowed,
                        pending,
                        message,
                        canvas_focus_requested,
                        #[cfg(test)]
                        test_canvas_rect,
                        #[cfg(test)]
                        test_canvas_id,
                        #[cfg(test)]
                        test_action_rects,
                        #[cfg(test)]
                        test_generic_preview_drawn,
                        #[cfg(test)]
                        test_native_inspector_visible,
                        #[cfg(test)]
                        test_generic_canvas_placeholder,
                    ),
                    ViewMode::Split => {
                        #[cfg(test)]
                        let mut source_id = None;
                        #[cfg(test)]
                        let mut source_rect = None;
                        #[cfg(test)]
                        let mut canvas_rect = None;
                        #[cfg(test)]
                        let mut canvas_id = None;
                        if ui.available_width() < SPLIT_TWO_COLUMN_MIN_WIDTH {
                            source_editor(
                                ui,
                                session,
                                render_document,
                                source_edit_allowed,
                                pending,
                                *focus_source && source_focus_allowed,
                                source_focus_requested,
                                #[cfg(test)]
                                &mut source_id,
                                #[cfg(test)]
                                &mut source_rect,
                                #[cfg(test)]
                                test_source_editor_enabled,
                                #[cfg(test)]
                                test_action_rects,
                                #[cfg(test)]
                                test_action_focus_ids,
                            );
                            ui.separator();
                            preview_workspace(
                                ui,
                                generic_family.is_some(),
                                generic_texture,
                                generic_canvas_placeholder,
                                graph,
                                cache.scene.as_ref(),
                                native_editable,
                                revision,
                                camera,
                                inspector_canvas_split_ratio,
                                selection,
                                selected_nodes,
                                property_draft,
                                drag,
                                editing_allowed,
                                pending,
                                message,
                                canvas_focus_requested,
                                #[cfg(test)]
                                &mut canvas_rect,
                                #[cfg(test)]
                                &mut canvas_id,
                                #[cfg(test)]
                                test_action_rects,
                                #[cfg(test)]
                                test_generic_preview_drawn,
                                #[cfg(test)]
                                test_native_inspector_visible,
                                #[cfg(test)]
                                test_generic_canvas_placeholder,
                            );
                        } else {
                            let available_width = ui.available_width();
                            let available_height = ui.available_height();
                            let (source_width, preview_width) = split_pane_widths(
                                available_width,
                                *source_preview_split_ratio,
                                SOURCE_PANE_MIN_WIDTH,
                                PREVIEW_PANE_MIN_WIDTH,
                                DEFAULT_SOURCE_PREVIEW_RATIO,
                            );
                            *source_preview_split_ratio = validated_split_ratio(
                                *source_preview_split_ratio,
                                available_width,
                                SPLITTER_WIDTH,
                                SOURCE_PANE_MIN_WIDTH,
                                PREVIEW_PANE_MIN_WIDTH,
                                DEFAULT_SOURCE_PREVIEW_RATIO,
                            );
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.allocate_ui_with_layout(
                                    egui::vec2(source_width, available_height),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        source_editor(
                                            ui,
                                            session,
                                            render_document,
                                            source_edit_allowed,
                                            pending,
                                            *focus_source && source_focus_allowed,
                                            source_focus_requested,
                                            #[cfg(test)]
                                            &mut source_id,
                                            #[cfg(test)]
                                            &mut source_rect,
                                            #[cfg(test)]
                                            test_source_editor_enabled,
                                            #[cfg(test)]
                                            test_action_rects,
                                            #[cfg(test)]
                                            test_action_focus_ids,
                                        );
                                    },
                                );
                                let (splitter_rect, splitter_response) = draw_splitter(
                                    ui,
                                    "source-preview",
                                    available_height,
                                    editing_allowed,
                                );
                                #[cfg(test)]
                                test_action_rects.push((
                                    "source_preview_splitter".into(),
                                    splitter_rect,
                                    ui.clip_rect(),
                                ));
                                if editing_allowed
                                    && splitter_response.dragged_by(egui::PointerButton::Primary)
                                    && let Some(pointer) = splitter_response.interact_pointer_pos()
                                {
                                    *source_preview_split_ratio = validated_split_ratio(
                                        split_ratio_from_pointer(
                                            pointer.x,
                                            splitter_rect.left(),
                                            source_width,
                                            available_width,
                                        ),
                                        available_width,
                                        SPLITTER_WIDTH,
                                        SOURCE_PANE_MIN_WIDTH,
                                        PREVIEW_PANE_MIN_WIDTH,
                                        DEFAULT_SOURCE_PREVIEW_RATIO,
                                    );
                                }
                                ui.allocate_ui_with_layout(
                                    egui::vec2(preview_width, available_height),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        preview_workspace(
                                            ui,
                                            generic_family.is_some(),
                                            generic_texture,
                                            generic_canvas_placeholder,
                                            graph,
                                            cache.scene.as_ref(),
                                            native_editable,
                                            revision,
                                            camera,
                                            inspector_canvas_split_ratio,
                                            selection,
                                            selected_nodes,
                                            property_draft,
                                            drag,
                                            editing_allowed,
                                            pending,
                                            message,
                                            canvas_focus_requested,
                                            #[cfg(test)]
                                            &mut canvas_rect,
                                            #[cfg(test)]
                                            &mut canvas_id,
                                            #[cfg(test)]
                                            test_action_rects,
                                            #[cfg(test)]
                                            test_generic_preview_drawn,
                                            #[cfg(test)]
                                            test_native_inspector_visible,
                                            #[cfg(test)]
                                            test_generic_canvas_placeholder,
                                        );
                                    },
                                );
                            });
                        }
                        #[cfg(test)]
                        {
                            *test_source_editor_id = source_id;
                            *test_source_editor_rect = source_rect;
                            *test_canvas_rect = canvas_rect;
                            *test_canvas_id = canvas_id;
                        }
                    }
                }
            });
        draw_effects
    }

    fn draw_visual_body(
        &mut self,
        ui: &mut egui::Ui,
        actions_allowed: bool,
        closing_requested: bool,
        pending: &mut Option<PendingMutation>,
        document_intent: &mut Option<DocumentIntent>,
        canvas_focus_requested: &mut Option<egui::Id>,
    ) -> DrawBodyEffects {
        #[cfg(test)]
        if !closing_requested && pending.is_none() {
            *pending = self.test_pending_mutation.take();
        }
        #[cfg(test)]
        if !closing_requested && document_intent.is_none() {
            *document_intent = self.test_pending_document_intent.take();
        }
        #[cfg(test)]
        {
            self.test_body_rendered = true;
        }

        let pending_transition = self.documents.pending_transition().cloned();
        let path = self.documents.path().map(Path::to_path_buf);
        let visual_presentation = self.visual_projection();
        let scene_snapshot = visual_presentation.as_ref().map(|(scene, _)| scene.clone());
        let projection = visual_presentation.map(|(_, projection)| projection);
        let visual_revision = self.documents.visual_revision().unwrap_or(0);
        let visual_identity = self.render_document.clone();
        let drawio_review_current = self
            .drawio_review
            .as_ref()
            .is_some_and(|review| review.scope().matches(self, ui.ctx().viewport_id()));
        let mut visual_asset_picker_requested = None;
        if let Some(scene) = scene_snapshot.as_deref() {
            self.sync_visual_asset_scope(
                scene,
                visual_revision,
                pending_transition.is_some(),
                closing_requested,
            );
            self.poll_visual_asset_work();
            if pending_transition.is_none() && !closing_requested {
                self.start_visual_asset_hydration(ui.ctx(), scene, visual_revision);
            }
            self.upload_visual_asset_textures(ui.ctx());
        }
        let focused_at_start = ui.memory(|memory| memory.focused());
        let previous_menu_scope = self.active_menu_scope.clone();
        let document_actions_allowed = actions_allowed
            && !closing_requested
            && pending_transition.is_none()
            && !egui::Popup::is_any_open(ui.ctx());
        let visual_edits_allowed = document_actions_allowed && pending.is_none();
        let visual_draft_active = self.visual_property_draft.is_some();
        let visual_canvas_editing_allowed = visual_edits_allowed && !visual_draft_active;
        let structural_edits_allowed = visual_edits_allowed && !visual_draft_active;
        let mut effects = DrawBodyEffects::default();
        let mut selection_state = VisualSelectionState {
            objects: self
                .visual_selected_objects
                .iter()
                .copied()
                .filter(|id| {
                    scene_snapshot
                        .as_ref()
                        .is_some_and(|scene| scene.objects.iter().any(|object| object.id == *id))
                })
                .collect(),
            primary: self.visual_selection,
        };
        if selection_state
            .primary
            .is_none_or(|primary| !selection_state.objects.contains(&primary))
        {
            selection_state.primary = selection_state.objects.last().copied();
        }
        let mut selected_object = self
            .visual_selection
            .filter(|id| {
                scene_snapshot
                    .as_ref()
                    .is_some_and(|scene| scene.objects.iter().any(|object| object.id == *id))
            })
            .or(selection_state.primary);
        let mut selected_preset = self.visual_selected_preset.filter(|id| {
            scene_snapshot
                .as_ref()
                .is_some_and(|scene| scene.presets.iter().any(|preset| preset.id == *id))
        });
        let mut working_object = selected_object.and_then(|id| {
            let saved_draft = self.visual_property_draft.as_ref().filter(|draft| {
                draft.document == visual_identity
                    && draft.basis_revision == visual_revision
                    && draft.object.id == id
            });
            saved_draft.map(|draft| draft.object.clone()).or_else(|| {
                scene_snapshot
                    .as_ref()?
                    .objects
                    .iter()
                    .find(|object| object.id == id)
                    .cloned()
            })
        });
        let mut staged_operations: Option<(Vec<VisualOperation>, Option<VisualObjectId>)> = None;
        let mut stage_history: Option<bool> = None;
        let mut clear_visual_draft = false;
        let mut canvas_interaction_authorized = false;
        let _visual_scroll = egui::ScrollArea::vertical()
            .id_salt("diagram-studio-visual-document-scroll")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading("Diagram Studio");
                    ui.strong("Visual document");
                    ui.separator();
                    let viewer = ui.add_enabled(
                        document_actions_allowed,
                        egui::Button::new(if self.viewer_open {
                            "Focus viewer"
                        } else {
                            "Open viewer"
                        }),
                    );
                    #[cfg(test)]
                    self.test_action_rects.push((
                        "open_viewer".into(),
                        viewer.rect,
                        ui.clip_rect(),
                    ));
                    #[cfg(test)]
                    self.test_action_enabled.push(("open_viewer".into(), viewer.enabled() && ui.is_visible() && !ui.is_sizing_pass()));
                    if viewer.clicked() {
                        effects.open_viewer_requested = true;
                    }
                    let _drawio_menu = ui.menu_button("Draw.io", |menu_ui| {
                        let menu_allowed = actions_allowed
                            && !closing_requested
                            && pending_transition.is_none()
                            && pending.is_none()
                            && !visual_draft_active;
                        let available = self.drawio_work.is_none()
                            && self.drawio_review.is_none()
                            && self.visual_asset_work.is_none()
                            && self.completed_visual_asset_import.is_none()
                            && self.completed_visual_export.is_none();
                        let import = menu_ui.add_enabled(
                            menu_allowed && available,
                            egui::Button::new("Import diagram…"),
                        );
                        #[cfg(test)]
                        self.test_action_rects.push((
                            "drawio_import".into(),
                            import.rect,
                            menu_ui.clip_rect(),
                        ));
                        if import.clicked() && document_intent.is_none() {
                            *document_intent = Some(DocumentIntent::DrawioImport);
                            menu_ui.close();
                        }
                        let export = menu_ui.add_enabled(
                            menu_allowed && available,
                            egui::Button::new("Export diagram…"),
                        );
                        #[cfg(test)]
                        self.test_action_rects.push((
                            "drawio_export".into(),
                            export.rect,
                            menu_ui.clip_rect(),
                        ));
                        if export.clicked() && document_intent.is_none() {
                            *document_intent = Some(DocumentIntent::DrawioExport);
                            menu_ui.close();
                        }
                    });
                    #[cfg(test)]
                    self.test_action_rects.push((
                        "menu_drawio".into(),
                        _drawio_menu.response.rect,
                        ui.clip_rect(),
                    ));
                    for (_name, label, intent) in [
                        ("document_new", "New Mermaid", DocumentIntent::New),
                        ("document_new_visual", "New Visual", DocumentIntent::NewVisual),
                        ("document_open", "Open…", DocumentIntent::Open),
                        ("document_save", "Save", DocumentIntent::Save),
                        ("document_save_as", "Save As…", DocumentIntent::SaveAs),
                        ("document_close", "Close", DocumentIntent::Close),
                    ] {
                        let response = ui.add_enabled(
                            document_actions_allowed,
                            egui::Button::new(label),
                        );
                        #[cfg(test)]
                        self.test_action_rects.push((
                            _name.to_owned(),
                            response.rect,
                            ui.clip_rect(),
                        ));
                        if response.clicked() && document_intent.is_none() {
                            *document_intent = Some(intent);
                        }
                    }
                    let template_allowed = actions_allowed && !closing_requested && pending_transition.is_none() && pending.is_none() && !visual_draft_active;
                    let mut template_item_ids = Vec::new();
                    let mut template_sizing_pass = false;
                    let templates = ui.add_enabled_ui(template_allowed, |ui| ui.menu_button("Visual templates", |menu_ui| {
                        template_sizing_pass = menu_ui.is_sizing_pass();
                        for template in crate::diagram::VisualAuthoringTemplate::ALL {
                            let response = menu_ui.add_enabled(template_allowed, egui::Button::new(template.name()));
                            if response.enabled() { template_item_ids.push(response.id); }
                            #[cfg(test)] {
                                let action = format!("visual_authoring_template_{}", template.slug());
                                self.test_action_enabled.push((action.clone(), response.enabled() && menu_ui.is_visible() && !menu_ui.is_sizing_pass()));
                                self.test_action_rects.push((action.clone(), response.rect, menu_ui.clip_rect()));
                                self.test_action_focus_ids.push((action, response.id));
                            }
                            if response.clicked() && document_intent.is_none() {
                                *document_intent = Some(DocumentIntent::ApplyVisualTemplate(template));
                                menu_ui.close();
                            }
                        }
                    })).inner;
                    #[cfg(test)] {
                        self.test_action_rects.push(("menu_visual_templates".into(), templates.response.rect, ui.clip_rect()));
                        self.test_action_focus_ids.push(("menu_visual_templates".into(), templates.response.id));
                    }
                    let template_popup_id = egui::Popup::default_response_id(&templates.response);
                    if egui::Popup::is_id_open(ui.ctx(), template_popup_id) {
                        effects.active_menu_scope = Some(MenuFocusScope { popup_id: template_popup_id, trigger_id: templates.response.id, item_ids: template_item_ids, sizing_pass: template_sizing_pass });
                    }
                    let export_busy = self.visual_asset_work.is_some()
                        || self.completed_visual_asset_import.is_some()
                        || self.completed_visual_export.is_some()
                        || self.drawio_work.is_some()
                        || self.drawio_review.is_some();
                    let export_allowed = actions_allowed
                        && !closing_requested
                        && pending_transition.is_none()
                        && pending.is_none()
                        && !visual_draft_active;
                    let mut export_items = Vec::new();
                    let mut export_sizing = false;
                    let export_menu = ui.add_enabled_ui(export_allowed, |ui| {
                        let (response, inner) = egui::containers::menu::MenuButton::new("Visual export")
                            .config(egui::containers::menu::MenuConfig::new().close_behavior(
                                egui::PopupCloseBehavior::CloseOnClickOutside,
                            ))
                            .ui(ui, |menu_ui| {
                                export_sizing = menu_ui.is_sizing_pass();
                                menu_ui.label("Scale");
                                for (scale, label, _name) in ExportScale::ALL {
                                    let response = menu_ui.selectable_label(self.export_scale == scale, label);
                                    if response.enabled() { export_items.push(response.id); }
                                    #[cfg(test)] {
                                        let action = format!("visual_export_scale_{_name}");
                                        self.test_action_enabled.push((action.clone(), response.enabled() && menu_ui.is_visible() && !menu_ui.is_sizing_pass()));
                                        self.test_action_rects.push((action, response.rect, menu_ui.clip_rect()));
                                    }
                                    if response.clicked() { effects.visual_export_scale_requested = Some(scale); }
                                }
                                menu_ui.separator();
                                menu_ui.label("Background");
                                for (_name, label, color) in [
                                    ("transparent", "Transparent", None),
                                    ("white", "White", Some(VisualColor::rgba(255, 255, 255, 255))),
                                    ("charcoal", "Charcoal", Some(VisualColor::rgba(34, 39, 46, 255))),
                                ] {
                                    let response = menu_ui.selectable_label(self.visual_export_background == color, label);
                                    if response.enabled() { export_items.push(response.id); }
                                    #[cfg(test)] {
                                        let action = format!("visual_export_background_{_name}");
                                        self.test_action_enabled.push((action.clone(), response.enabled() && menu_ui.is_visible() && !menu_ui.is_sizing_pass()));
                                        self.test_action_rects.push((action, response.rect, menu_ui.clip_rect()));
                                    }
                                    if response.clicked() { effects.visual_export_background_requested = Some(color); }
                                }
                                let color = self.visual_export_background.unwrap_or(VisualColor::rgba(255, 255, 255, 255));
                                let mut rgba = [color.red, color.green, color.blue, color.alpha];
                                let color_response = menu_ui.color_edit_button_srgba_unmultiplied(&mut rgba);
                                if color_response.changed() {
                                    effects.visual_export_background_requested = Some(Some(VisualColor::rgba(rgba[0], rgba[1], rgba[2], rgba[3])));
                                }
                                menu_ui.separator();
                                for (_name, label, format) in [
                                    ("visual_export_svg", "Export SVG…", DiagramExportFormat::Svg),
                                    ("visual_export_png", "Export PNG…", DiagramExportFormat::Png),
                                ] {
                                    let response = menu_ui.add_enabled(!export_busy, egui::Button::new(label));
                                    if response.enabled() { export_items.push(response.id); }
                                    #[cfg(test)] {
                                        self.test_action_rects.push((_name.into(), response.rect, menu_ui.clip_rect()));
                                        self.test_action_enabled.push((_name.into(), response.enabled() && menu_ui.is_visible() && !menu_ui.is_sizing_pass()));
                                    }
                                    if response.clicked() && document_intent.is_none() {
                                        *document_intent = Some(DocumentIntent::Export(format, self.export_scale));
                                        menu_ui.close();
                                    }
                                }
                            });
                        egui::InnerResponse::new(inner.map(|result| result.inner), response)
                    }).inner;
                    #[cfg(test)] {
                        self.test_action_rects.push(("visual_export_options".into(), export_menu.response.rect, ui.clip_rect()));
                        self.test_action_focus_ids.push(("visual_export_options".into(), export_menu.response.id));
                    }
                    let export_popup = egui::Popup::default_response_id(&export_menu.response);
                    if egui::Popup::is_id_open(ui.ctx(), export_popup) {
                        effects.active_menu_scope = Some(MenuFocusScope {
                            popup_id: export_popup,
                            trigger_id: export_menu.response.id,
                            item_ids: export_items,
                            sizing_pass: export_sizing,
                        });
                    }
                    let export_cancellable = self.completed_visual_export.is_some()
                        || self.visual_asset_work.as_ref().is_some_and(|work| {
                            !work.retired && matches!(&work.kind, VisualAssetWorkKind::Export)
                        });
                    if export_cancellable {
                        let cancel_export = ui.add_enabled(export_allowed, egui::Button::new("Cancel export"));
                        effects.visual_export_cancel_pointer_down = cancel_export.is_pointer_button_down_on();
                        #[cfg(test)] {
                            self.test_action_rects.push(("visual_export_cancel".into(), cancel_export.rect, ui.clip_rect()));
                            self.test_action_enabled.push(("visual_export_cancel".into(), cancel_export.enabled()));
                        }
                        if cancel_export.clicked() {
                            effects.visual_export_cancel_requested = Some(VisualExportCancelFence {
                                host: self.owner_host.identity(),
                                document: visual_identity.clone(),
                                revision: visual_revision,
                                owner_generation: self.owner_generation,
                                owner_viewport_id: ui.ctx().viewport_id(),
                            });
                        }
                    }
                    if let Some(path) = path.as_deref() {
                        ui.weak(path.file_name().and_then(|name| name.to_str()).unwrap_or("diagram.mld"));
                    } else {
                        ui.weak("Untitled");
                    }
                    if self.documents.is_dirty() {
                        ui.weak("Modified");
                    }
                    let import_enabled = structural_edits_allowed
                        && self.visual_asset_work.is_none()
                        && self.completed_visual_asset_import.is_none()
                        && self.completed_visual_export.is_none()
                        && self.drawio_work.is_none()
                        && self.drawio_review.is_none();
                    let add_image = ui.add_enabled(
                        import_enabled,
                        egui::Button::new("Add image…"),
                    );
                    #[cfg(test)]
                    self.test_action_rects.push((
                        "visual_add_image".into(),
                        add_image.rect,
                        ui.clip_rect(),
                    ));
                    if add_image.clicked() && document_intent.is_none() && pending.is_none()
                    {
                        let owner_viewport_id = ui.ctx().viewport_id();
                        visual_asset_picker_requested = Some(VisualAssetImportFence {
                            document: visual_identity.clone(),
                            revision: visual_revision,
                            owner_generation: self.owner_generation,
                            owner_viewport_id,
                            start_pass: ViewportPassProvenance::current(
                                ui.ctx(),
                                owner_viewport_id,
                            ),
                            start_pass_accepted: false,
                        });
                    }
                });

                draw_drawio_workflow_panel(
                    ui,
                    self.drawio_work.as_ref(),
                    self.drawio_review.as_ref(),
                    drawio_review_current,
                    structural_edits_allowed && !visual_draft_active,
                    document_intent,
                    #[cfg(test)]
                    &mut self.test_action_rects,
                );

                effects.recent_panel = draw_recent_documents(
                    ui,
                    self.recent_index.as_ref(),
                    self.recent_index_loaded,
                    self.recent_index_error.as_deref(),
                    self.recent_index_status.as_deref(),
                    &mut self.recent_search,
                    self.recent_panel_open,
                    document_actions_allowed,
                    document_intent,
                    #[cfg(test)]
                    &mut self.test_action_rects,
                );

                let mut label = self.documents.visual_label().unwrap_or_default().to_owned();
                let label_editor_id = ui.make_persistent_id("diagram-studio-visual-label-editor");
                clear_text_edit_undoer(ui.ctx(), label_editor_id);
                let label_response = ui.add_enabled(
                    document_actions_allowed && pending.is_none() && !visual_draft_active,
                    egui::TextEdit::singleline(&mut label)
                        .id(label_editor_id)
                        .hint_text("Visual canvas label")
                        .desired_width(f32::INFINITY),
                );
                #[cfg(test)]
                self.test_action_rects.push((
                    "visual_label".into(),
                    label_response.rect,
                    ui.clip_rect(),
                ));
                #[cfg(test)]
                self.test_action_focus_ids
                    .push(("visual_label".into(), label_response.id));
                if label_response.changed() && pending.is_none() {
                    *pending = Some(PendingMutation::ReplaceVisualLabel {
                        label: label.clone(),
                        expected_document: visual_identity.clone(),
                        expected_revision: visual_revision,
                    });
                }

                ui.label("Canvas (drag objects or handles; wheel or middle-drag to navigate)");
                if let Some((id, error)) = self
                    .visual_asset_errors
                    .iter()
                    .min_by_key(|(id, _)| id.as_str())
                {
                    ui.colored_label(
                        egui::Color32::from_rgb(185, 65, 65),
                        format!("Image {} is unavailable: {error}", &id.as_str()[..8]),
                    );
                }
                let mut camera = self.camera;
                let navigation_controls_allowed = visual_edits_allowed && self.visual_gesture.is_none();
                ui.horizontal(|ui| {
                    let zoom_out = ui.add_enabled(navigation_controls_allowed, egui::Button::new("−"));
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_zoom_out".into(), zoom_out.rect, ui.clip_rect()));
                    if zoom_out.clicked() {
                        camera.zoom = (camera.zoom / CAMERA_KEYBOARD_ZOOM_FACTOR)
                            .clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
                        camera.fit_next = false;
                    }
                    ui.label(format!("{:.0}%", camera.zoom * 100.0));
                    let zoom_in = ui.add_enabled(navigation_controls_allowed, egui::Button::new("+"));
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_zoom_in".into(), zoom_in.rect, ui.clip_rect()));
                    if zoom_in.clicked() {
                        camera.zoom = (camera.zoom * CAMERA_KEYBOARD_ZOOM_FACTOR)
                            .clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
                        camera.fit_next = false;
                    }
                    let fit = ui.add_enabled(navigation_controls_allowed, egui::Button::new("Fit"));
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_fit".into(), fit.rect, ui.clip_rect()));
                    if fit.clicked() {
                        camera.fit_next = true;
                    }
                    let zoom_100 = ui.add_enabled(navigation_controls_allowed, egui::Button::new("100%"));
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_zoom_100".into(), zoom_100.rect, ui.clip_rect()));
                    if zoom_100.clicked() {
                        camera.zoom = 1.0;
                        camera.fit_next = false;
                    }
                });
                let canvas_size = egui::vec2(ui.available_width().clamp(240.0, 1_400.0), 340.0);
                let focused_widget_before_canvas = ui.memory(|memory| memory.focused());
                let (canvas_response, canvas_painter) = ui.allocate_painter(
                    canvas_size,
                    egui::Sense::click_and_drag(),
                );
                let canvas_rect = canvas_response.rect;
                let canvas_mapping = pure_canvas_mapping(canvas_rect.shrink(12.0), camera);
                ui.painter().rect_filled(
                    canvas_rect,
                    4.0,
                    egui::Color32::from_gray(249),
                );
                let canvas_painter = canvas_painter.with_clip_rect(canvas_mapping_rect(canvas_mapping));
                let base_projection = projection
                    .as_ref()
                    .and_then(|result| result.as_ref().ok())
                    .map(Arc::as_ref);
                let canvas_bounds = base_projection.and_then(|projection| projection.bounds);
                if camera.fit_next && let Some(bounds) = canvas_bounds {
                    fit_camera(&mut camera, bounds, canvas_rect.shrink(12.0));
                }
                let minimap_rect = visual_minimap_screen_rect(canvas_rect, canvas_painter.clip_rect());
                let minimap_response = minimap_rect.map(|rect| {
                    ui.interact(
                        rect,
                        ui.make_persistent_id(("diagram-visual-minimap", canvas_response.id)),
                        egui::Sense::click_and_drag(),
                    )
                });
                #[cfg(test)]
                if let Some(rect) = minimap_rect {
                    self.test_action_rects
                        .push(("visual_minimap".into(), rect, ui.clip_rect()));
                }
                let minimap_camera = camera;
                let minimap_interacting = handle_visual_minimap_drag(ui, minimap_rect, &mut camera, &mut self.visual_editor_minimap_drag, navigation_controls_allowed, self.owner_host.identity(), self.owner_generation, || {
                    let (rect, projected) = (minimap_rect?, base_projection?);
                    minimap_layout_for_camera(projected, canvas_rect.shrink(12.0), canvas_painter.clip_rect(), rect, minimap_camera)
                });
                let mut gesture = self.visual_gesture.clone();
                handle_visual_canvas_camera_input(
                    ui,
                    &canvas_response,
                    canvas_painter.clip_rect(),
                    minimap_rect,
                    &mut camera,
                    navigation_controls_allowed && !minimap_interacting,
                );
                if navigation_controls_allowed && !minimap_interacting
                    && !minimap_response.as_ref().is_some_and(egui::Response::hovered)
                {
                    viewer_canvas_shortcuts(ui, canvas_response.id, &mut camera);
                }

                let base_painted_prefix = base_projection.map_or(0, visual_painted_item_prefix);
                let widget_focus_allows_canvas = ui
                    .memory(|memory| memory.focused())
                    .is_none_or(|focused| focused == canvas_response.id);
                canvas_interaction_authorized =
                    visual_canvas_editing_allowed && widget_focus_allows_canvas;
                let pointer_press_authorized = self
                    .visual_canvas_press_authorized
                    .unwrap_or(canvas_interaction_authorized);
                let canvas_editing_allowed = canvas_interaction_authorized
                    && pointer_press_authorized
                    && gesture.is_none();
                if !visual_canvas_editing_allowed || !widget_focus_allows_canvas {
                    gesture = None;
                    self.visual_canvas_press_authorized = None;
                }
                let mut start_gesture = None;
                let mut pointer_selection = None;
                let modifiers = ui.ctx().input(|input| input.modifiers);
                let extend_selection = modifiers.command || modifiers.shift;
                let mut mapping = pure_canvas_mapping(canvas_rect.shrink(12.0), camera);
                mapping.clip = canvas_rect_to_scene_rect(
                    canvas_painter.clip_rect().intersect(canvas_rect.shrink(12.0)),
                );
                let (primary_pressed, pointer_position) = ui.ctx().input(|input| {
                    (
                        input.pointer.button_pressed(egui::PointerButton::Primary),
                        input.pointer.interact_pos(),
                    )
                });
                if primary_pressed
                    && pointer_position.is_some_and(|point| {
                        mapping.contains_screen_point(scene_point_from_screen(point))
                            && !minimap_rect.is_some_and(|mini| mini.contains(point))
                    })
                {
                    self.visual_canvas_press_authorized = Some(
                        visual_canvas_editing_allowed
                            && focused_widget_before_canvas
                                .is_none_or(|focused| focused == canvas_response.id),
                    );
                }
                let current_projection_for_hit = base_projection;
                let current_scene_for_hit = scene_snapshot.as_deref();
                let selected_for_hit = selection_state.objects.clone();
                let handles = match (current_scene_for_hit, current_projection_for_hit) {
                    (Some(scene), Some(projection)) => visual_canvas_selection_handles(
                        scene,
                        projection,
                        &selected_for_hit,
                        base_painted_prefix,
                        mapping,
                    ),
                    _ => Vec::new(),
                };

                if document_actions_allowed
                    && canvas_response.drag_started_by(egui::PointerButton::Primary)
                    && ui.ctx().input(|input| input.pointer.press_origin()).is_some_and(|point| {
                        mapping.contains_screen_point(scene_point_from_screen(point))
                            && !minimap_rect.is_some_and(|mini| mini.contains(point))
                    })
                {
                    *canvas_focus_requested = Some(canvas_response.id);
                }

                if visual_edits_allowed
                    && canvas_editing_allowed
                    && canvas_response.drag_started_by(egui::PointerButton::Primary)
                    && let Some(pointer_start) = ui.ctx().input(|input| input.pointer.press_origin())
                    && mapping.contains_screen_point(scene_point_from_screen(pointer_start))
                    && !minimap_rect.is_some_and(|mini| mini.contains(pointer_start))
                    && let (Some(scene), Some(projected)) =
                        (current_scene_for_hit, current_projection_for_hit)
                {
                    let hit = visual_canvas_hit_test(
                        projected,
                        base_painted_prefix,
                        &handles,
                        mapping,
                        scene_point_from_screen(pointer_start),
                        9.0,
                    );
                    let next_selection = reduce_canvas_selection(
                        &selection_state,
                        hit,
                        extend_selection,
                        true,
                    );
                    let mode = canvas_gesture_mode(
                        scene,
                        hit,
                        &next_selection,
                        projected,
                        base_painted_prefix,
                        extend_selection,
                    );
                    if let Some(mode) = mode
                        && let Some(resolved) = crate::diagram::visual_canvas_resolve_membership(
                            scene,
                            &next_selection.objects,
                        )
                    {
                        let transformable_ids =
                            resolved.expanded.into_iter().collect::<BTreeSet<_>>();
                        let initial_objects = scene
                            .objects
                            .iter()
                            .filter(|object| {
                                transformable_ids.contains(&object.id)
                                    && !matches!(
                                        &object.kind,
                                        VisualObjectKind::Connector { .. }
                                    )
                            })
                            .cloned()
                            .collect::<Vec<_>>();
                        if !matches!(mode, VisualGestureMode::Transform(_)) || !initial_objects.is_empty() {
                            pointer_selection = Some(next_selection.clone());
                            start_gesture = Some(VisualGesture {
                                document: visual_identity.clone(),
                                revision: visual_revision,
                                owner_generation: self.owner_generation,
                                owner_viewport_id: ui.ctx().viewport_id(),
                                start_pass: ViewportPassProvenance::current(
                                    ui.ctx(),
                                    ui.ctx().viewport_id(),
                                ),
                                start_pass_accepted: false,
                                mode,
                                mapping,
                                pointer_start,
                                pointer_start_world: mapping.screen_to_world(scene_point_from_screen(pointer_start)),
                                initial_selection: next_selection,
                                initial_objects: initial_objects.clone(),
                                last_pointer: pointer_start,
                                preview_objects: initial_objects,
                                preview_scene: None,
                                preview_projection: None,
                                preview_guides: Vec::new(),
                                did_move: false,
                            });
                        }
                    }
                    *canvas_focus_requested = Some(canvas_response.id);
                }
                if document_actions_allowed
                    && canvas_response.clicked_by(egui::PointerButton::Primary)
                    && !canvas_response.dragged_by(egui::PointerButton::Primary)
                    && let Some(pointer) = canvas_response.interact_pointer_pos()
                    && mapping.contains_screen_point(scene_point_from_screen(pointer))
                    && !minimap_rect.is_some_and(|mini| mini.contains(pointer))
                {
                    *canvas_focus_requested = Some(canvas_response.id);
                    if canvas_editing_allowed
                    && let (Some(_scene), Some(projected)) =
                        (current_scene_for_hit, current_projection_for_hit)
                    {
                        let hit = visual_canvas_hit_test(
                            projected,
                            base_painted_prefix,
                            &handles,
                            mapping,
                            scene_point_from_screen(pointer),
                            9.0,
                        );
                        if !matches!(hit, CanvasHit::Handle(_)) {
                            pointer_selection = Some(reduce_canvas_selection(
                                &selection_state,
                                hit,
                                extend_selection,
                                false,
                            ));
                        }
                    }
                }
                if let Some(next_selection) = pointer_selection {
                    selection_state = next_selection;
                    selected_object = selection_state.primary;
                    working_object = None;
                    clear_visual_draft = true;
                }

                if gesture
                    .as_ref()
                    .is_some_and(|active| active.mapping != mapping)
                {
                    gesture = None;
                    self.visual_canvas_press_authorized = None;
                }

                if let Some(mut active) = start_gesture.or_else(|| gesture.take()) {
                    let (pointer, button_down, button_released) = ui.ctx().input(|input| {
                        (
                            input.pointer.interact_pos(),
                            input.pointer.button_down(egui::PointerButton::Primary),
                            input.pointer.button_released(egui::PointerButton::Primary),
                        )
                    });
                    let pointer = clamp_canvas_screen_point(
                        active.mapping,
                        pointer.unwrap_or(active.last_pointer),
                    );
                    let moved = pointer.distance(active.pointer_start) > 1.5;
                    active.did_move |= moved;
                    let pointer_changed = pointer != active.last_pointer;
                    if active.did_move
                        && (button_down || button_released)
                        && canvas_interaction_authorized
                    {
                        match active.mode {
                            VisualGestureMode::Marquee { .. } => {}
                            VisualGestureMode::Transform(mode) if pointer_changed => {
                                let mut current_world = active
                                    .mapping
                                    .screen_to_world(scene_point_from_screen(pointer));
                                active.preview_guides.clear();
                                if mode == VisualTransformMode::Translate
                                    && let Some(projected) = current_projection_for_hit
                                {
                                    let raw_delta = ScenePoint {
                                        x: current_world.x - active.pointer_start_world.x,
                                        y: current_world.y - active.pointer_start_world.y,
                                    };
                                    if let Some(scene) = scene_snapshot.as_deref()
                                        && let Some(snapped) = visual_canvas_snap_translation_delta(
                                            scene,
                                            projected,
                                            &active.initial_selection.objects,
                                            base_painted_prefix,
                                            raw_delta,
                                            active.mapping.zoom,
                                        )
                                    {
                                        current_world = ScenePoint {
                                            x: active.pointer_start_world.x + snapped.delta.x,
                                            y: active.pointer_start_world.y + snapped.delta.y,
                                        };
                                        active.preview_guides = snapped.guides;
                                    }
                                }
                                if let Some(objects) = preview_visual_transforms(
                                    &active.initial_objects,
                                    mode,
                                    active.pointer_start_world,
                                    current_world,
                                ) {
                                    if let Some(mut preview_scene) = scene_snapshot.as_ref().map(|scene| scene.as_ref().clone()) {
                                        for object in &objects {
                                            if let Some(saved) = preview_scene.objects.iter_mut().find(|saved| saved.id == object.id) {
                                                *saved = object.clone();
                                            }
                                        }
                                            if let Ok(projected) = project_visual_scene(&preview_scene) {
                                                active.preview_projection = Some(Arc::new(projected));
                                                active.preview_scene = Some(Arc::new(preview_scene));
                                                active.preview_objects = objects;
                                            } else {
                                                active.preview_projection = None;
                                                active.preview_scene = None;
                                                active.preview_objects = active.initial_objects.clone();
                                                active.preview_guides.clear();
                                            }
                                    }
                                } else {
                                        active.preview_projection = None;
                                        active.preview_scene = None;
                                        active.preview_objects = active.initial_objects.clone();
                                        active.preview_guides.clear();
                                }
                            }
                            VisualGestureMode::Transform(_) => {}
                        }
                        active.last_pointer = pointer;
                    }
                    let released = button_released || canvas_response.drag_stopped_by(egui::PointerButton::Primary);
                    if released {
                        if active.did_move && active.start_pass_accepted && visual_edits_allowed {
                            match active.mode {
                                VisualGestureMode::Marquee { extend } => {
                                    if let Some(projected) = current_projection_for_hit {
                                        let end_world = active
                                            .mapping
                                            .screen_to_world(scene_point_from_screen(pointer));
                                        let marquee = SceneRect {
                                            min: active.pointer_start_world,
                                            max: end_world,
                                        };
                                        let mut objects = if extend { active.initial_selection.objects } else { BTreeSet::new() };
                                        let hits = visual_canvas_marquee_selection(projected, base_painted_prefix, marquee);
                                        objects.extend(hits);
                                        let primary = objects.last().copied();
                                        selection_state = VisualSelectionState { objects, primary };
                                        selected_object = primary;
                                        working_object = None;
                                    }
                                }
                                VisualGestureMode::Transform(_) => {
                                    let operations = active
                                        .preview_objects
                                        .iter()
                                        .filter(|preview| active.initial_objects.iter().find(|initial| initial.id == preview.id).is_some_and(|initial| initial != *preview))
                                        .cloned()
                                        .map(VisualOperation::ReplaceObject)
                                        .collect::<Vec<_>>();
                                    if !operations.is_empty()
                                        && operations.len() <= MAX_VISUAL_TRANSACTION_OPERATIONS
                                    {
                                        *pending = Some(PendingMutation::VisualGestureTransaction {
                                            operations,
                                            expected_document: active.document,
                                            expected_revision: active.revision,
                                            owner_generation: active.owner_generation,
                                            owner_viewport_id: active.owner_viewport_id,
                                            start_pass: active.start_pass,
                                            start_pass_accepted: active.start_pass_accepted,
                                        });
                                    }
                                }
                            }
                        }
                        gesture = None;
                    } else if button_down && visual_edits_allowed {
                        gesture = Some(active);
                    } else {
                        gesture = None;
                    }
                }
                if !ui.ctx().input(|input| {
                    input.pointer.button_down(egui::PointerButton::Primary)
                }) {
                    self.visual_canvas_press_authorized = None;
                }

                let active_scene = gesture
                    .as_ref()
                    .and_then(|gesture| gesture.preview_scene.as_deref())
                    .or(scene_snapshot.as_deref());
                let active_projection = gesture
                    .as_ref()
                    .and_then(|gesture| gesture.preview_projection.as_deref())
                    .or(base_projection);
                let painted_selection = gesture
                    .as_ref()
                    .map_or(&selection_state.objects, |gesture| &gesture.initial_selection.objects);
                let mut paint_report = VisualPaintReport::default();
                match active_projection {
                    Some(active_projection) => {
                        paint_report = paint_visual_projection(
                            &canvas_painter,
                            active_projection,
                            canvas_rect,
                            camera,
                            painted_selection,
                            &self.visual_asset_textures,
                            &self.visual_asset_errors,
                        );
                        if visual_canvas_editing_allowed && !visual_draft_active
                            && let Some(active_scene) = active_scene
                        {
                            let prefix = paint_report.visible_item_prefix;
                            let active_handles = visual_canvas_selection_handles(
                                active_scene,
                                active_projection,
                                painted_selection,
                                prefix,
                                mapping,
                            );
                            paint_visual_canvas_handles(&canvas_painter, &active_handles, mapping);
                        }
                    }
                    None => {
                        if let Some(Some(error)) = projection.as_ref().map(|result| result.as_ref().err()) {
                            canvas_painter.rect_filled(canvas_rect, 4.0, egui::Color32::from_gray(248));
                            canvas_painter.text(
                                canvas_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                error.to_string(),
                                egui::FontId::proportional(14.0),
                                egui::Color32::DARK_RED,
                            );
                        }
                    }
                }
                if let Some(active) = gesture.as_ref()
                    && matches!(active.mode, VisualGestureMode::Marquee { .. })
                    && active.did_move
                {
                    let pointer = clamp_canvas_screen_point(
                        active.mapping,
                        ui.ctx().input(|input| input.pointer.interact_pos()).unwrap_or(active.last_pointer),
                    );
                    let marquee = egui::Rect::from_two_pos(active.pointer_start, pointer);
                    canvas_painter.rect_stroke(
                        marquee.intersect(canvas_mapping_rect(active.mapping)),
                        0.0,
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 112, 201)),
                        egui::StrokeKind::Inside,
                    );
                    #[cfg(test)]
                    {
                        self.test_marquee_order = ui.ctx().graphics(|graphics| {
                            let list = graphics.get(canvas_painter.layer_id())?;
                            let entries: Vec<_> = list.all_entries().collect();
                            let background = entries.iter().rposition(|entry| matches!(&entry.shape, egui::Shape::Rect(rect) if rect.fill == egui::Color32::from_gray(249) && rect.rect == canvas_rect))?;
                            let outline = entries.iter().rposition(|entry| matches!(&entry.shape, egui::Shape::Rect(rect) if rect.stroke.color == egui::Color32::from_rgb(36, 112, 201)))?;
                            Some((background, outline, entries[outline].clip_rect))
                        });
                    }
                }

                if let Some(active) = gesture.as_ref() {
                    paint_visual_canvas_guides(&canvas_painter, &active.preview_guides, mapping);
                }
                if let (Some(rect), Some(active_projection)) = (minimap_rect, active_projection)
                    && let Some(minimap) = minimap_layout_for_camera(
                        active_projection,
                        canvas_rect.shrink(12.0),
                        canvas_painter.clip_rect(),
                        rect,
                        camera,
                    )
                {
                    paint_visual_minimap(&canvas_painter, rect, &minimap);
                }
                self.visual_gesture = gesture;
                self.camera = camera;
                if let Some(Ok(projection)) = projection.as_ref()
                    && (projection.simplified_dash_paths > 0
                        || projection.truncated_text_items > 0
                        || projection.connector_route_fallbacks > 0
                        || paint_report.omitted_items > 0
                        || paint_report.suppressed_text_lines > 0)
                {
                    ui.weak(format!(
                        "{} large dashed outline(s) simplified; {} connector route(s) shown in orange fallback; {} text item(s) shortened, {} small text line(s) omitted; {} item(s) omitted to keep the canvas responsive.",
                        projection.simplified_dash_paths,
                        projection.connector_route_fallbacks,
                        projection.truncated_text_items,
                        paint_report.suppressed_text_lines,
                        paint_report.omitted_items
                    ));
                }
                #[cfg(test)]
                {
                    self.test_visual_canvas_rect = Some(canvas_rect);
                    self.test_visual_canvas_id = Some(canvas_response.id);
                    self.test_action_focus_ids
                        .push(("visual_canvas".into(), canvas_response.id));
                    self.test_visual_items_painted = paint_report.painted_items;
                    self.test_visual_items_omitted = paint_report.omitted_items;
                    self.test_action_rects.push((
                        "visual_canvas".into(),
                        canvas_rect,
                        ui.clip_rect(),
                    ));
                }

                let resolved_layout_selection = scene_snapshot.as_deref().and_then(|scene| {
                    crate::diagram::visual_canvas_resolve_membership(
                        scene,
                        &selection_state.objects,
                    )
                });
                let transformable_selection_count = resolved_layout_selection
                    .as_ref()
                    .map_or(0, |selection| selection.roots.len());
                let layout_owner_available = scene_snapshot
                    .as_deref()
                    .zip(base_projection)
                    .is_some_and(|(scene, projection)| {
                        resolved_layout_selection.is_some()
                            && visual_canvas_selection_bounds(
                                scene,
                                projection,
                                &selection_state.objects,
                                base_painted_prefix,
                            )
                            .is_some()
                    });
                let layout_actions_allowed = visual_canvas_editing_allowed
                    && document_intent.is_none()
                    && self.visual_gesture.is_none()
                    && layout_owner_available;
                let align_enabled = layout_actions_allowed && transformable_selection_count >= 2;
                let distribute_enabled = layout_actions_allowed && transformable_selection_count >= 3;
                ui.horizontal_wrapped(|ui| {
                    ui.weak("Align");
                    for (_name, label, alignment) in [
                        ("visual_align_left", "Left", VisualAlign::Left),
                        ("visual_align_center_x", "Center X", VisualAlign::HorizontalCenter),
                        ("visual_align_right", "Right", VisualAlign::Right),
                        ("visual_align_top", "Top", VisualAlign::Top),
                        ("visual_align_center_y", "Center Y", VisualAlign::VerticalCenter),
                        ("visual_align_bottom", "Bottom", VisualAlign::Bottom),
                    ] {
                        let response = ui.add_enabled(align_enabled, egui::Button::new(label));
                        #[cfg(test)]
                        self.test_action_rects
                            .push((_name.into(), response.rect, ui.clip_rect()));
                        if response.clicked()
                            && pending.is_none()
                            && let (Some(scene), Some(projected)) =
                                (scene_snapshot.as_deref(), base_projection)
                        {
                            match visual_canvas_align_selection(
                                scene,
                                projected,
                                &selection_state.objects,
                                projected.items.len(),
                                alignment,
                            ) {
                                Some(operations) if operations.is_empty() => {}
                                Some(operations)
                                    if operations.len()
                                        <= MAX_VISUAL_TRANSACTION_OPERATIONS =>
                                {
                                    staged_operations = Some((operations, None));
                                }
                                _ => {
                                    *pending = Some(PendingMutation::VisualLayoutFailure {
                                        message: "Could not align the selected objects because their transformed outlines are unavailable or outside the supported range.".into(),
                                        expected_document: visual_identity.clone(),
                                        expected_revision: visual_revision,
                                    });
                                }
                            }
                        }
                    }
                    ui.weak("Distribute");
                    for (_name, label, distribution) in [
                        ("visual_distribute_horizontal", "Horizontal", VisualDistribution::Horizontal),
                        ("visual_distribute_vertical", "Vertical", VisualDistribution::Vertical),
                    ] {
                        let response = ui.add_enabled(
                            distribute_enabled,
                            egui::Button::new(label),
                        );
                        #[cfg(test)]
                        self.test_action_rects
                            .push((_name.into(), response.rect, ui.clip_rect()));
                        if response.clicked()
                            && pending.is_none()
                            && let (Some(scene), Some(projected)) =
                                (scene_snapshot.as_deref(), base_projection)
                        {
                            match visual_canvas_distribute_selection(
                                scene,
                                projected,
                                &selection_state.objects,
                                projected.items.len(),
                                distribution,
                            ) {
                                Some(operations) if operations.is_empty() => {}
                                Some(operations)
                                    if operations.len()
                                        <= MAX_VISUAL_TRANSACTION_OPERATIONS =>
                                {
                                    staged_operations = Some((operations, None));
                                }
                                _ => {
                                    *pending = Some(PendingMutation::VisualLayoutFailure {
                                        message: "Could not distribute the selected objects because their transformed outlines are unavailable or outside the supported range.".into(),
                                        expected_document: visual_identity.clone(),
                                        expected_revision: visual_revision,
                                    });
                                }
                            }
                        }
                    }
                });

                ui.separator();
                ui.add_enabled_ui(visual_edits_allowed && !visual_draft_active, |ui| {
                    ui.horizontal_wrapped(|ui| {
                    ui.label("Object");
                    let _object_combo = egui::ComboBox::from_id_salt("diagram-visual-object-selection")
                        .selected_text(selected_object.map_or_else(
                            || "None".to_owned(),
                            |id| format!("#{}", id.get()),
                        ))
                        .show_ui(ui, |ui| {
                            if let Some(scene) = scene_snapshot.as_ref() {
                                for object in &scene.objects {
                                    let _response = ui.selectable_value(
                                        &mut selected_object,
                                        Some(object.id),
                                        visual_object_label(object),
                                    );
                                    #[cfg(test)]
                                    self.test_action_rects.push((
                                        format!("visual_object_option_{}", object.id.get()),
                                        _response.rect,
                                        ui.clip_rect(),
                                    ));
                                }
                            }
                        });
                    #[cfg(test)]
                    self.test_action_rects.push((
                        "visual_object_selection".into(),
                        _object_combo.response.rect,
                        ui.clip_rect(),
                    ));
                    let before_selection = self.visual_selection;
                    if selected_object != before_selection {
                        working_object = selected_object.and_then(|id| {
                            scene_snapshot
                                .as_ref()?
                                .objects
                                .iter()
                                .find(|object| object.id == id)
                                .cloned()
                        });
                        clear_visual_draft = true;
                    }
                    let selected_shape = ui.add_enabled_ui(structural_edits_allowed, |ui| {
                        let mut shape = self.visual_create_shape;
                        egui::ComboBox::from_id_salt("diagram-visual-create-shape")
                            .selected_text(visual_shape_label(shape))
                            .show_ui(ui, |ui| {
                                for candidate in visual_primitive_shapes() {
                                    ui.selectable_value(
                                        &mut shape,
                                        candidate,
                                        visual_shape_label(candidate),
                                    );
                                }
                            });
                        shape
                    });
                    self.visual_create_shape = selected_shape.inner;

                    let add_shape = ui.add_enabled(structural_edits_allowed, egui::Button::new("Add shape"));
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_add_shape".into(), add_shape.rect, ui.clip_rect()));
                    if add_shape.clicked() && document_intent.is_none() {
                        staged_operations = Some((
                            vec![VisualOperation::AddObject(VisualObjectDraft {
                                layer_id: DEFAULT_LAYER_ID,
                                transform: next_visual_object_transform(scene_snapshot.as_deref()),
                                style: VisualStyle::default(),
                                kind: VisualObjectKind::Primitive {
                                    shape: self.visual_create_shape,
                                },
                            })],
                            None,
                        ));
                    }
                    let add_text = ui.add_enabled(structural_edits_allowed, egui::Button::new("Add text"));
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_add_text".into(), add_text.rect, ui.clip_rect()));
                    if add_text.clicked() && document_intent.is_none() {
                        staged_operations = Some((
                            vec![VisualOperation::AddObject(VisualObjectDraft {
                                layer_id: DEFAULT_LAYER_ID,
                                transform: next_visual_object_transform(scene_snapshot.as_deref()),
                                style: VisualStyle::default(),
                                kind: VisualObjectKind::Text {
                                    text: "New text".into(),
                                },
                            })],
                            None,
                        ));
                    }
                    let add_container = ui.add_enabled(
                        structural_edits_allowed,
                        egui::Button::new("Add container"),
                    );
                    #[cfg(test)]
                    self.test_action_rects.push((
                        "visual_add_container".into(),
                        add_container.rect,
                        ui.clip_rect(),
                    ));
                    if add_container.clicked() && document_intent.is_none() {
                        staged_operations = Some((
                            vec![VisualOperation::AddObject(VisualObjectDraft {
                                layer_id: DEFAULT_LAYER_ID,
                                transform: next_visual_frame_transform(
                                    scene_snapshot.as_deref(),
                                ),
                                style: visual_frame_style(),
                                kind: VisualObjectKind::Container {
                                    label: "Container".into(),
                                },
                            })],
                            None,
                        ));
                    }
                    let group_roots = scene_snapshot.as_deref()
                        .map_or_else(Vec::new, |scene| visual_group_roots(scene, &self.visual_selected_objects));
                    let group = ui.add_enabled(
                        structural_edits_allowed && !group_roots.is_empty(),
                        egui::Button::new("Group selection"),
                    );
                    #[cfg(test)]
                    {
                        self.test_visual_group_enabled = group.enabled();
                        self.test_action_rects.push(("visual_group_selection".into(), group.rect, ui.clip_rect()));
                    }
                    if group.clicked() && document_intent.is_none() {
                        if let (Some(scene), Some(projection)) = (scene_snapshot.as_deref(), base_projection) {
                            let group_selection = group_roots.iter().copied().collect();
                            if let Some(bounds) = visual_canvas_selection_bounds(scene, projection, &group_selection, visual_painted_item_prefix(projection)) {
                                staged_operations = Some((vec![VisualOperation::Group {
                                    frame: VisualObjectDraft {
                                        layer_id: DEFAULT_LAYER_ID,
                                        transform: VisualTransform { x: bounds.center().x, y: bounds.center().y, width: (bounds.max.x - bounds.min.x + 40.0).max(40.0), height: (bounds.max.y - bounds.min.y + 64.0).max(64.0), rotation_degrees: 0.0 },
                                        style: visual_frame_style(),
                                        kind: VisualObjectKind::Container { label: "Group".into() },
                                    },
                                    members: group_roots,
                                }], None));
                            }
                        }
                    }
                    let ungroup_id = scene_snapshot.as_ref().and_then(|scene| self.visual_selection.and_then(|id| scene.objects.iter().find(|object| object.id == id && matches!(object.kind, VisualObjectKind::Container { .. } | VisualObjectKind::Swimlane { .. })).map(|object| object.id)));
                    let ungroup = ui.add_enabled(structural_edits_allowed && ungroup_id.is_some(), egui::Button::new("Ungroup"));
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_ungroup".into(), ungroup.rect, ui.clip_rect()));
                    if ungroup.clicked() && document_intent.is_none() {
                        if let Some(id) = ungroup_id { staged_operations = Some((vec![VisualOperation::Ungroup(id)], None)); }
                    }
                    let add_horizontal_swimlane = ui.add_enabled(
                        structural_edits_allowed,
                        egui::Button::new("Add horizontal lane"),
                    );
                    #[cfg(test)]
                    self.test_action_rects.push((
                        "visual_add_horizontal_swimlane".into(),
                        add_horizontal_swimlane.rect,
                        ui.clip_rect(),
                    ));
                    if add_horizontal_swimlane.clicked() && document_intent.is_none() {
                        staged_operations = Some((
                            vec![VisualOperation::AddObject(VisualObjectDraft {
                                layer_id: DEFAULT_LAYER_ID,
                                transform: next_visual_frame_transform(
                                    scene_snapshot.as_deref(),
                                ),
                                style: visual_frame_style(),
                                kind: VisualObjectKind::Swimlane {
                                    orientation: VisualSwimlaneOrientation::Horizontal,
                                    label: "Horizontal lane".into(),
                                },
                            })],
                            None,
                        ));
                    }
                    let add_vertical_swimlane = ui.add_enabled(
                        structural_edits_allowed,
                        egui::Button::new("Add vertical lane"),
                    );
                    #[cfg(test)]
                    self.test_action_rects.push((
                        "visual_add_vertical_swimlane".into(),
                        add_vertical_swimlane.rect,
                        ui.clip_rect(),
                    ));
                    if add_vertical_swimlane.clicked() && document_intent.is_none() {
                        staged_operations = Some((
                            vec![VisualOperation::AddObject(VisualObjectDraft {
                                layer_id: DEFAULT_LAYER_ID,
                                transform: next_visual_frame_transform(
                                    scene_snapshot.as_deref(),
                                ),
                                style: visual_frame_style(),
                                kind: VisualObjectKind::Swimlane {
                                    orientation: VisualSwimlaneOrientation::Vertical,
                                    label: "Vertical lane".into(),
                                },
                            })],
                            None,
                        ));
                    }
                    let connector_endpoints = scene_snapshot
                        .as_ref()
                        .map(|scene| {
                            scene
                                .objects
                                .iter()
                                .filter(|object| {
                                    !matches!(
                                        &object.kind,
                                        VisualObjectKind::Connector { .. }
                                    )
                                })
                                .take(2)
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let add_connector = ui.add_enabled(
                        structural_edits_allowed && connector_endpoints.len() == 2,
                        egui::Button::new("Add connector"),
                    );
                    #[cfg(test)]
                    self.test_action_rects.push((
                        "visual_add_connector".into(),
                        add_connector.rect,
                        ui.clip_rect(),
                    ));
                    if add_connector.clicked() && document_intent.is_none() {
                        let source = &connector_endpoints[0];
                        let target = &connector_endpoints[1];
                        let source_point = ScenePoint {
                            x: source.transform.x,
                            y: source.transform.y,
                        };
                        let target_point = ScenePoint {
                            x: target.transform.x,
                            y: target.transform.y,
                        };
                        staged_operations = Some((
                            vec![VisualOperation::AddObject(VisualObjectDraft {
                                layer_id: DEFAULT_LAYER_ID,
                                transform: VisualTransform::default(),
                                style: VisualStyle::default(),
                                kind: VisualObjectKind::Connector {
                                    connector: VisualConnector {
                                        from: VisualConnectorEndpoint {
                                            object_id: source.id,
                                            anchor: nearest_visual_anchor(source, target_point),
                                        },
                                        to: VisualConnectorEndpoint {
                                            object_id: target.id,
                                            anchor: nearest_visual_anchor(target, source_point),
                                        },
                                        route: VisualConnectorRoute::Elbow,
                                        direction: VisualConnectorDirection::Forward,
                                        label: String::new(),
                                    },
                                },
                            })],
                            None,
                        ));
                    }
                    let add_layer = ui.add_enabled(structural_edits_allowed, egui::Button::new("Add layer"));
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_add_layer".into(), add_layer.rect, ui.clip_rect()));
                    if add_layer.clicked() && document_intent.is_none() {
                        let index = scene_snapshot.as_ref().map_or(0, |scene| scene.layers.len());
                        staged_operations = Some((
                            vec![VisualOperation::AddLayer {
                                name: format!("Layer {}", index + 1),
                                to_index: index,
                            }],
                            None,
                        ));
                    }
                    if let (Some(scene), Some(object)) = (
                        scene_snapshot.as_ref(),
                        selected_object.and_then(|id| {
                            scene_snapshot
                                .as_ref()?
                                .objects
                                .iter()
                                .find(|object| object.id == id)
                        }),
                    ) && let Some(layer_index) = scene
                        .layers
                        .iter()
                        .position(|layer| layer.id == object.layer_id)
                    {
                        let send_layer_back = ui.add_enabled(
                            structural_edits_allowed && layer_index > 0,
                            egui::Button::new("Layer back"),
                        );
                        #[cfg(test)]
                        self.test_action_rects.push((
                            "visual_layer_back".into(),
                            send_layer_back.rect,
                            ui.clip_rect(),
                        ));
                        if send_layer_back.clicked()
                            && document_intent.is_none()
                            && pending.is_none()
                        {
                            staged_operations = Some((
                                vec![VisualOperation::MoveLayer {
                                    id: object.layer_id,
                                    to_index: layer_index - 1,
                                }],
                                None,
                            ));
                        }
                        let bring_layer_forward = ui.add_enabled(
                            structural_edits_allowed && layer_index + 1 < scene.layers.len(),
                            egui::Button::new("Layer forward"),
                        );
                        #[cfg(test)]
                        self.test_action_rects.push((
                            "visual_layer_forward".into(),
                            bring_layer_forward.rect,
                            ui.clip_rect(),
                        ));
                        if bring_layer_forward.clicked()
                            && document_intent.is_none()
                            && pending.is_none()
                        {
                            staged_operations = Some((
                                vec![VisualOperation::MoveLayer {
                                    id: object.layer_id,
                                    to_index: layer_index + 1,
                                }],
                                None,
                            ));
                        }
                    }
                    let selected_primitives = scene_snapshot
                        .as_ref()
                        .map(|scene| {
                            scene
                                .objects
                                .iter()
                                .filter(|object| selection_state.objects.contains(&object.id))
                                .filter(|object| {
                                    matches!(object.kind, VisualObjectKind::Primitive { .. })
                                })
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let create_preset_enabled = structural_edits_allowed
                        && !selected_primitives.is_empty()
                        && selected_primitives.len() <= MAX_VISUAL_PRESET_COMPONENTS;
                    let add_preset = ui.add_enabled(
                        create_preset_enabled,
                        egui::Button::new("Create preset from selection"),
                    );
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_create_preset".into(), add_preset.rect, ui.clip_rect()));
                    if add_preset.clicked() && document_intent.is_none() {
                        let count = scene_snapshot.as_ref().map_or(0, |scene| scene.presets.len());
                        match VisualPresetDraft::from_primitives(
                            format!("Preset {}", count + 1),
                            &selected_primitives,
                        ) {
                            Ok(preset) => {
                                staged_operations =
                                    Some((vec![VisualOperation::AddPreset(preset)], None));
                            }
                            Err(error) => {
                                self.message = Some(StudioNotice::error(error.to_string()));
                            }
                        }
                    }
                    if let Some(scene) = scene_snapshot.as_ref() {
                        if selected_preset.is_none() {
                            selected_preset = scene.presets.first().map(|preset| preset.id);
                        }
                        egui::ComboBox::from_id_salt("diagram-visual-preset-selection")
                            .selected_text(selected_preset.and_then(|id| {
                                scene.presets.iter().find(|preset| preset.id == id)
                            }).map_or_else(|| "No preset".to_owned(), |preset| preset.name.clone()))
                            .show_ui(ui, |ui| {
                                for preset in &scene.presets {
                                    ui.selectable_value(
                                        &mut selected_preset,
                                        Some(preset.id),
                                        preset.name.clone(),
                                    );
                                }
                            });
                    }
                    let add_instance = ui.add_enabled(
                        structural_edits_allowed && selected_preset.is_some(),
                        egui::Button::new("Add preset instance"),
                    );
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_add_preset_instance".into(), add_instance.rect, ui.clip_rect()));
                    if add_instance.clicked() && document_intent.is_none()
                        && let Some(preset_id) = selected_preset
                    {
                        staged_operations = Some((
                            vec![VisualOperation::AddObject(VisualObjectDraft {
                                layer_id: DEFAULT_LAYER_ID,
                                transform: next_visual_preset_instance_transform(
                                    scene_snapshot.as_deref(),
                                ),
                                style: VisualStyle::default(),
                                kind: VisualObjectKind::PresetInstance { preset_id },
                            })],
                            None,
                        ));
                    }
                    let undo = ui.add_enabled(
                        structural_edits_allowed && self.documents.can_undo(),
                        egui::Button::new("Undo"),
                    );
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_undo".into(), undo.rect, ui.clip_rect()));
                    if undo.clicked() && document_intent.is_none() {
                        stage_history = Some(false);
                    }
                    let redo = ui.add_enabled(
                        structural_edits_allowed && self.documents.can_redo(),
                        egui::Button::new("Redo"),
                    );
                    #[cfg(test)]
                    self.test_action_rects.push(("visual_redo".into(), redo.rect, ui.clip_rect()));
                    if redo.clicked() && document_intent.is_none() {
                        stage_history = Some(true);
                    }
                    });
                });

                if let (Some(scene), Some(selected_id), Some(object)) = (
                    scene_snapshot.as_ref(),
                    selected_object,
                    working_object.as_mut(),
                ) {
                    ui.add_enabled_ui(visual_edits_allowed, |ui| {
                    egui::CollapsingHeader::new("Object properties")
                        .default_open(true)
                        .show(ui, |ui| {
                        let selected_is_connector = matches!(
                            &object.kind,
                            VisualObjectKind::Connector { .. }
                        );
                        if selected_id != crate::diagram::TITLE_OBJECT_ID
                            && !selected_is_connector
                        {
                            let current_parent = scene.parent_by_child.get(&selected_id).copied();
                            let mut parent_target = self.visual_membership_target;
                            ui.add_enabled_ui(!visual_draft_active, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(current_parent.map_or_else(
                                        || "Frame: none".to_owned(),
                                        |id| format!("Frame: #{}", id.get()),
                                    ));
                                    let _parent_combo = egui::ComboBox::from_id_salt((
                                        "diagram-visual-membership-parent",
                                        selected_id,
                                    ))
                                    .selected_text(parent_target
                                        .and_then(|id| {
                                            scene.objects.iter().find(|candidate| candidate.id == id)
                                        })
                                        .map_or_else(|| "Choose frame".to_owned(), visual_object_label))
                                    .show_ui(ui, |ui| {
                                        for candidate in scene.objects.iter().filter(|candidate| {
                                            matches!(
                                                &candidate.kind,
                                                VisualObjectKind::Container { .. }
                                                    | VisualObjectKind::Swimlane { .. }
                                            ) && visual_frame_can_parent(scene, selected_id, candidate.id)
                                        }) {
                                            let _response = ui.selectable_value(
                                                &mut parent_target,
                                                Some(candidate.id),
                                                visual_object_label(candidate),
                                            );
                                            #[cfg(test)]
                                            self.test_action_rects.push((
                                                format!("visual_parent_option_{}", candidate.id.get()),
                                                _response.rect,
                                                ui.clip_rect(),
                                            ));
                                        }
                                    });
                                    #[cfg(test)]
                                    self.test_action_rects.push((
                                        "visual_membership_parent".into(),
                                        _parent_combo.response.rect,
                                        ui.clip_rect(),
                                    ));
                                    let assign = ui.add_enabled(
                                        visual_edits_allowed
                                            && pending.is_none()
                                            && parent_target.is_some_and(|parent| {
                                                Some(parent) != current_parent
                                            }),
                                        egui::Button::new("Assign / reparent"),
                                    );
                                    #[cfg(test)]
                                    self.test_action_rects.push((
                                        "visual_assign_parent".into(),
                                        assign.rect,
                                        ui.clip_rect(),
                                    ));
                                    if assign.clicked()
                                        && pending.is_none()
                                        && document_intent.is_none()
                                        && let Some(parent) = parent_target
                                    {
                                        staged_operations = Some((
                                            vec![VisualOperation::SetParent {
                                                child: selected_id,
                                                parent: Some(parent),
                                            }],
                                            None,
                                        ));
                                    }
                                    let detach = ui.add_enabled(
                                        visual_edits_allowed
                                            && pending.is_none()
                                            && current_parent.is_some(),
                                        egui::Button::new("Detach from frame"),
                                    );
                                    #[cfg(test)]
                                    self.test_action_rects.push((
                                        "visual_detach_parent".into(),
                                        detach.rect,
                                        ui.clip_rect(),
                                    ));
                                    if detach.clicked()
                                        && pending.is_none()
                                        && document_intent.is_none()
                                    {
                                        staged_operations = Some((
                                            vec![VisualOperation::SetParent {
                                                child: selected_id,
                                                parent: None,
                                            }],
                                            None,
                                        ));
                                    }
                                });
                            });
                            self.visual_membership_target = parent_target;
                        }
                        let populated_frame = matches!(
                            &object.kind,
                            VisualObjectKind::Container { .. } | VisualObjectKind::Swimlane { .. }
                        ) && scene
                            .parent_by_child
                            .values()
                            .any(|parent| *parent == selected_id);
                        if matches!(
                            &object.kind,
                            VisualObjectKind::Container { .. } | VisualObjectKind::Swimlane { .. }
                        ) {
                            ui.weak("Removing a frame promotes direct members without moving them; connectors attached to the frame are removed.");
                        }
                        if !matches!(&object.kind, VisualObjectKind::Connector { .. }) {
                            ui.add_enabled_ui(!populated_frame, |ui| ui.horizontal_wrapped(|ui| {
                                ui.label("Position / size");
                                ui.add(egui::DragValue::new(&mut object.transform.x).prefix("x ").speed(1.0));
                                ui.add(egui::DragValue::new(&mut object.transform.y).prefix("y ").speed(1.0));
                                ui.add(egui::DragValue::new(&mut object.transform.width).prefix("w ").speed(1.0));
                                ui.add(egui::DragValue::new(&mut object.transform.height).prefix("h ").speed(1.0));
                                ui.add(egui::DragValue::new(&mut object.transform.rotation_degrees).prefix("° ").speed(1.0));
                            }));
                            if populated_frame {
                                ui.weak("Use the canvas group handles to transform this frame with its members.");
                            }
                        }
                        match &mut object.kind {
                            VisualObjectKind::Text { text } => {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Ink");
                                    edit_visual_color(ui, &mut object.style.border);
                                    ui.add(egui::DragValue::new(&mut object.style.opacity).prefix("Opacity ").speed(0.02));
                                    ui.add(egui::DragValue::new(&mut object.style.font_size).prefix("Font ").speed(0.5));
                                    visual_font_family_picker(ui, &mut object.style.font_family);
                                });
                                ui.label("Text");
                                ui.add(egui::TextEdit::multiline(text).desired_rows(2));
                            }
                            VisualObjectKind::Container { label } => {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Header fill");
                                    edit_visual_color(ui, &mut object.style.fill);
                                    ui.label("Border / text");
                                    edit_visual_color(ui, &mut object.style.border);
                                    ui.add(egui::DragValue::new(&mut object.style.border_width).prefix("Stroke ").speed(0.25));
                                    ui.add(egui::DragValue::new(&mut object.style.opacity).prefix("Opacity ").speed(0.02));
                                    ui.add(egui::DragValue::new(&mut object.style.font_size).prefix("Label font ").speed(0.5));
                                });
                                let label_id = ui.make_persistent_id((
                                    "diagram-visual-frame-label",
                                    self.render_document.clone(),
                                    selected_id,
                                ));
                                clear_text_edit_undoer(ui.ctx(), label_id);
                                let _label_edit = ui.add(
                                    egui::TextEdit::singleline(label).id(label_id),
                                );
                                #[cfg(test)]
                                self.test_action_rects.push((
                                    "visual_frame_label".into(),
                                    _label_edit.rect,
                                    ui.clip_rect(),
                                ));
                            }
                            VisualObjectKind::Swimlane { orientation, label } => {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Header fill");
                                    edit_visual_color(ui, &mut object.style.fill);
                                    ui.label("Border / text");
                                    edit_visual_color(ui, &mut object.style.border);
                                    ui.add(egui::DragValue::new(&mut object.style.border_width).prefix("Stroke ").speed(0.25));
                                    ui.add(egui::DragValue::new(&mut object.style.opacity).prefix("Opacity ").speed(0.02));
                                    ui.add(egui::DragValue::new(&mut object.style.font_size).prefix("Label font ").speed(0.5));
                                    let _orientation_combo = egui::ComboBox::from_id_salt("diagram-visual-swimlane-orientation")
                                        .selected_text(match orientation {
                                            VisualSwimlaneOrientation::Horizontal => "Horizontal",
                                            VisualSwimlaneOrientation::Vertical => "Vertical",
                                        })
                                        .show_ui(ui, |ui| {
                                            let _horizontal = ui.selectable_value(
                                                orientation,
                                                VisualSwimlaneOrientation::Horizontal,
                                                "Horizontal",
                                            );
                                            #[cfg(test)]
                                            self.test_action_rects.push((
                                                "visual_swimlane_horizontal".into(),
                                                _horizontal.rect,
                                                ui.clip_rect(),
                                            ));
                                            let _vertical = ui.selectable_value(
                                                orientation,
                                                VisualSwimlaneOrientation::Vertical,
                                                "Vertical",
                                            );
                                            #[cfg(test)]
                                            self.test_action_rects.push((
                                                "visual_swimlane_vertical".into(),
                                                _vertical.rect,
                                                ui.clip_rect(),
                                            ));
                                        });
                                    #[cfg(test)]
                                    self.test_action_rects.push((
                                        "visual_swimlane_orientation".into(),
                                        _orientation_combo.response.rect,
                                        ui.clip_rect(),
                                    ));
                                });
                                let label_id = ui.make_persistent_id((
                                    "diagram-visual-frame-label",
                                    self.render_document.clone(),
                                    selected_id,
                                ));
                                clear_text_edit_undoer(ui.ctx(), label_id);
                                let _label_edit = ui.add(
                                    egui::TextEdit::singleline(label).id(label_id),
                                );
                                #[cfg(test)]
                                self.test_action_rects.push((
                                    "visual_frame_label".into(),
                                    _label_edit.rect,
                                    ui.clip_rect(),
                                ));
                            }
                            VisualObjectKind::Primitive { .. } => {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Fill");
                                    edit_visual_color(ui, &mut object.style.fill);
                                    ui.label("Border");
                                    edit_visual_color(ui, &mut object.style.border);
                                    ui.add(egui::DragValue::new(&mut object.style.border_width).prefix("Stroke ").speed(0.25));
                                    ui.add(egui::DragValue::new(&mut object.style.opacity).prefix("Opacity ").speed(0.02));
                                    ui.label("Line");
                                    egui::ComboBox::from_id_salt("diagram-visual-line-style")
                                        .selected_text(visual_line_style_label(object.style.line_style))
                                        .show_ui(ui, |ui| {
                                            for style in [VisualLineStyle::Solid, VisualLineStyle::Dashed, VisualLineStyle::Dotted] {
                                                ui.selectable_value(&mut object.style.line_style, style, visual_line_style_label(style));
                                            }
                                        });
                                });
                            }
                            VisualObjectKind::ImageReference { .. } => {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Placeholder fill");
                                    edit_visual_color(ui, &mut object.style.fill);
                                    ui.label("Border / ink");
                                    edit_visual_color(ui, &mut object.style.border);
                                    ui.add(egui::DragValue::new(&mut object.style.border_width).prefix("Stroke ").speed(0.25));
                                    ui.add(egui::DragValue::new(&mut object.style.opacity).prefix("Opacity ").speed(0.02));
                                    ui.add(egui::DragValue::new(&mut object.style.font_size).prefix("Font ").speed(0.5));
                                    visual_font_family_picker(ui, &mut object.style.font_family);
                                    egui::ComboBox::from_id_salt("diagram-visual-image-line-style")
                                        .selected_text(visual_line_style_label(object.style.line_style))
                                        .show_ui(ui, |ui| {
                                            for style in [VisualLineStyle::Solid, VisualLineStyle::Dashed, VisualLineStyle::Dotted] {
                                                ui.selectable_value(&mut object.style.line_style, style, visual_line_style_label(style));
                                            }
                                        });
                                });
                                ui.weak("Image assets are not loaded in this checkpoint.");
                            }
                            VisualObjectKind::PresetInstance { .. } => {
                                ui.horizontal(|ui| {
                                    ui.label("Preset instance opacity");
                                    ui.add(egui::DragValue::new(&mut object.style.opacity).range(0.0..=1.0).speed(0.02));
                                });
                                ui.weak("Primitive styles are defined in the preset.");
                            }
                            VisualObjectKind::Connector { connector } => {
                                visual_connector_endpoint_editor(
                                    ui,
                                    "From",
                                    "diagram-visual-connector-from",
                                    "visual_connector_from",
                                    &mut connector.from,
                                    selected_id,
                                    &scene.objects,
                                    |option_id, action, rect, clip| {
                                        #[cfg(test)]
                                        self.test_action_rects.push((
                                            option_id.map_or_else(
                                                || action.to_owned(),
                                                |id| format!("{action}_object_{}", id.get()),
                                            ),
                                            rect,
                                            clip,
                                        ));
                                        #[cfg(not(test))]
                                        let _ = (option_id, action, rect, clip);
                                    },
                                );
                                visual_connector_endpoint_editor(
                                    ui,
                                    "To",
                                    "diagram-visual-connector-to",
                                    "visual_connector_to",
                                    &mut connector.to,
                                    selected_id,
                                    &scene.objects,
                                    |option_id, action, rect, clip| {
                                        #[cfg(test)]
                                        self.test_action_rects.push((
                                            option_id.map_or_else(
                                                || action.to_owned(),
                                                |id| format!("{action}_object_{}", id.get()),
                                            ),
                                            rect,
                                            clip,
                                        ));
                                        #[cfg(not(test))]
                                        let _ = (option_id, action, rect, clip);
                                    },
                                );
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Route");
                                    let _route_combo = egui::ComboBox::from_id_salt("diagram-visual-connector-route")
                                        .selected_text(visual_connector_route_label(connector.route))
                                        .show_ui(ui, |ui| {
                                            for route in [
                                                VisualConnectorRoute::Straight,
                                                VisualConnectorRoute::Elbow,
                                                VisualConnectorRoute::Curved,
                                            ] {
                                                let _response = ui.selectable_value(
                                                    &mut connector.route,
                                                    route,
                                                    visual_connector_route_label(route),
                                                );
                                                #[cfg(test)]
                                                self.test_action_rects.push((
                                                    format!(
                                                        "visual_connector_route_option_{}",
                                                        visual_connector_route_label(route).to_lowercase()
                                                    ),
                                                    _response.rect,
                                                    ui.clip_rect(),
                                                ));
                                            }
                                        });
                                    #[cfg(test)]
                                    self.test_action_rects.push((
                                        "visual_connector_route".into(),
                                        _route_combo.response.rect,
                                        ui.clip_rect(),
                                    ));
                                    ui.label("Direction");
                                    let _direction_combo = egui::ComboBox::from_id_salt("diagram-visual-connector-direction")
                                        .selected_text(visual_connector_direction_label(connector.direction))
                                        .show_ui(ui, |ui| {
                                            for direction in [
                                                VisualConnectorDirection::None,
                                                VisualConnectorDirection::Forward,
                                                VisualConnectorDirection::Reverse,
                                                VisualConnectorDirection::Both,
                                            ] {
                                                let _response = ui.selectable_value(
                                                    &mut connector.direction,
                                                    direction,
                                                    visual_connector_direction_label(direction),
                                                );
                                                #[cfg(test)]
                                                self.test_action_rects.push((
                                                    format!(
                                                        "visual_connector_direction_option_{}",
                                                        visual_connector_direction_label(direction)
                                                            .to_lowercase()
                                                            .replace(' ', "_")
                                                    ),
                                                    _response.rect,
                                                    ui.clip_rect(),
                                                ));
                                            }
                                        });
                                    #[cfg(test)]
                                    self.test_action_rects.push((
                                        "visual_connector_direction".into(),
                                        _direction_combo.response.rect,
                                        ui.clip_rect(),
                                    ));
                                });
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Line");
                                    egui::ComboBox::from_id_salt("diagram-visual-connector-line-style")
                                        .selected_text(visual_line_style_label(object.style.line_style))
                                        .show_ui(ui, |ui| {
                                            for style in [
                                                VisualLineStyle::Solid,
                                                VisualLineStyle::Dashed,
                                                VisualLineStyle::Dotted,
                                            ] {
                                                ui.selectable_value(
                                                    &mut object.style.line_style,
                                                    style,
                                                    visual_line_style_label(style),
                                                );
                                            }
                                        });
                                    ui.label("Ink");
                                    edit_visual_color(ui, &mut object.style.border);
                                    ui.add(egui::DragValue::new(&mut object.style.border_width).prefix("Stroke ").speed(0.25));
                                    ui.add(egui::DragValue::new(&mut object.style.opacity).prefix("Opacity ").speed(0.02));
                                });
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Label font");
                                    ui.add(egui::DragValue::new(&mut object.style.font_size).prefix("Size ").speed(0.5));
                                    visual_font_family_picker(ui, &mut object.style.font_family);
                                });
                                ui.label("Connector label");
                                let _label_edit = ui.add(
                                    egui::TextEdit::multiline(&mut connector.label)
                                        .desired_rows(2)
                                        .char_limit(MAX_VISUAL_CONNECTOR_LABEL_BYTES / 4),
                                );
                                #[cfg(test)]
                                self.test_action_rects.push((
                                    "visual_connector_label".into(),
                                    _label_edit.rect,
                                    ui.clip_rect(),
                                ));
                            }
                        }
                        if !matches!(
                            &object.kind,
                            VisualObjectKind::PresetInstance { .. }
                                | VisualObjectKind::Connector { .. }
                        ) {
                            let mut has_shadow = object.style.shadow.is_some();
                            if ui.checkbox(&mut has_shadow, "Shadow").changed() {
                                object.style.shadow = has_shadow.then_some(VisualShadow {
                                    offset_x: 4.0,
                                    offset_y: 4.0,
                                    blur: 5.0,
                                    color: VisualColor::rgba(0, 0, 0, 90),
                                });
                            }
                            if let Some(shadow) = object.style.shadow.as_mut() {
                                ui.horizontal_wrapped(|ui| {
                                    ui.add(egui::DragValue::new(&mut shadow.offset_x).prefix("Shadow x ").speed(0.5));
                                    ui.add(egui::DragValue::new(&mut shadow.offset_y).prefix("y ").speed(0.5));
                                    ui.add(egui::DragValue::new(&mut shadow.blur).prefix("Blur ").speed(0.5));
                                });
                            }
                        }
                        if let Some(scene_layer) = scene.layers.iter().find(|layer| layer.id == object.layer_id) {
                            egui::ComboBox::from_id_salt("diagram-visual-object-layer")
                                .selected_text(scene_layer.name.clone())
                                .show_ui(ui, |ui| {
                                    for layer in &scene.layers {
                                        ui.selectable_value(&mut object.layer_id, layer.id, layer.name.clone());
                                    }
                                });
                        }
                        let object_index = scene
                            .objects
                            .iter()
                            .position(|candidate| candidate.id == selected_id)
                            .unwrap_or(0);
                        let same_layer_indices = scene
                            .objects
                            .iter()
                            .enumerate()
                            .filter(|(_, candidate)| candidate.layer_id == object.layer_id)
                            .map(|(index, _)| index)
                            .collect::<Vec<_>>();
                        let layer_position = same_layer_indices
                            .iter()
                            .position(|index| *index == object_index)
                            .unwrap_or(0);
                        let back_target = layer_position
                            .checked_sub(1)
                            .and_then(|position| same_layer_indices.get(position).copied());
                        let forward_target = same_layer_indices.get(layer_position + 1).copied();
                        ui.horizontal(|ui| {
                            let back = ui.add_enabled(structural_edits_allowed && back_target.is_some(), egui::Button::new("Send back"));
                            #[cfg(test)]
                            self.test_action_rects.push(("visual_send_back".into(), back.rect, ui.clip_rect()));
                            if back.clicked()
                                && pending.is_none()
                                && document_intent.is_none()
                                && let Some(to_index) = back_target
                            {
                                staged_operations = Some((vec![VisualOperation::MoveObject { id: selected_id, to_index }], None));
                            }
                            let forward = ui.add_enabled(structural_edits_allowed && forward_target.is_some(), egui::Button::new("Bring forward"));
                            #[cfg(test)]
                            self.test_action_rects.push(("visual_bring_forward".into(), forward.rect, ui.clip_rect()));
                            if forward.clicked()
                                && pending.is_none()
                                && document_intent.is_none()
                                && let Some(to_index) = forward_target
                            {
                                staged_operations = Some((vec![VisualOperation::MoveObject { id: selected_id, to_index }], None));
                            }
                            let remove = ui.add_enabled(structural_edits_allowed && selected_id != crate::diagram::TITLE_OBJECT_ID, egui::Button::new("Remove object"));
                            #[cfg(test)]
                            self.test_action_rects.push(("visual_remove_object".into(), remove.rect, ui.clip_rect()));
                            if remove.clicked() && pending.is_none() && document_intent.is_none() {
                                staged_operations = Some((vec![VisualOperation::RemoveObject(selected_id)], None));
                            }
                            let changed = scene.objects.iter().find(|candidate| candidate.id == selected_id).is_some_and(|saved| *saved != *object);
                            let apply = ui.add_enabled(visual_edits_allowed && changed, egui::Button::new("Apply properties"));
                            #[cfg(test)]
                            self.test_action_rects.push(("visual_apply_properties".into(), apply.rect, ui.clip_rect()));
                            if apply.clicked() && pending.is_none() && document_intent.is_none() {
                                if let Some(operations) = crate::diagram::visual_canvas_property_transform_operations(scene, object) {
                                    staged_operations = Some((operations, Some(selected_id)));
                                } else {
                                    self.message = Some(StudioNotice::error("Frame resize must be uniform and stay within geometry limits."));
                                }
                            }
                            let cancel = ui.add_enabled(visual_edits_allowed, egui::Button::new("Cancel property edit"));
                            #[cfg(test)]
                            self.test_action_rects.push(("visual_cancel_properties".into(), cancel.rect, ui.clip_rect()));
                            if cancel.clicked() {
                                clear_visual_draft = true;
                            }
                        });
                    });
                    });
                }

                if let Some(message) = self.message.as_ref() {
                    match message {
                        StudioNotice::Info(text) => {
                            ui.label(text);
                        }
                        StudioNotice::Error(text) => {
                            ui.colored_label(egui::Color32::from_rgb(185, 65, 65), text);
                        }
                    }
                }

                if let Some(transition) = pending_transition.as_ref() {
                    let description = match transition {
                        PendingDiagramTransition::New => "starting a new Mermaid document".to_owned(),
                        PendingDiagramTransition::NewVisual => "starting a new visual document".to_owned(),
                        PendingDiagramTransition::Open(path) => format!("opening {}", path.display()),
                        PendingDiagramTransition::ImportMermaid(file) => format!("importing {}", file.path().display()),
                        PendingDiagramTransition::CopySource(_) => "copying Mermaid source from Markdown".to_owned(),
                        PendingDiagramTransition::ApplyTemplate(template) => format!("using the {} template", template.name()),
                        PendingDiagramTransition::ApplyVisualTemplate(template) => format!("using the {} visual template", template.name()),
                        PendingDiagramTransition::Close => "closing this document".to_owned(),
                    };
                    ui.group(|ui| {
                        ui.label(format!("Save changes before {description}?"));
                        ui.horizontal_wrapped(|ui| {
                            for (_name, label, decision) in [
                                ("decision_save", "Save", TransitionDecision::Save),
                                ("decision_discard", "Discard", TransitionDecision::Discard),
                                ("decision_cancel", "Cancel", TransitionDecision::Cancel),
                            ] {
                                let response = ui.add_enabled(actions_allowed && !closing_requested, egui::Button::new(label));
                                #[cfg(test)]
                                self.test_action_rects.push((_name.into(), response.rect, ui.clip_rect()));
                                if response.clicked() && document_intent.is_none() {
                                    *document_intent = Some(DocumentIntent::ResolvePending(decision));
                                }
                            }
                        });
                    });
                }
            });
        #[cfg(test)]
        { self.test_visual_scroll_rect = Some(_visual_scroll.inner_rect); }
        if selected_object != selection_state.primary {
            selection_state.objects.clear();
            if let Some(id) = selected_object {
                selection_state.objects.insert(id);
            }
            selection_state.primary = selected_object;
        }
        self.visual_selected_preset = selected_preset;
        if clear_visual_draft {
            self.visual_property_draft = None;
        } else if let (Some(scene), Some(selected_id), Some(object)) =
            (scene_snapshot.as_ref(), selected_object, working_object)
            && let Some(original) = scene
                .objects
                .iter()
                .find(|candidate| candidate.id == selected_id)
        {
            if object == *original {
                self.visual_property_draft = None;
            } else {
                self.visual_property_draft = Some(VisualPropertyDraft {
                    document: visual_identity.clone(),
                    basis_revision: visual_revision,
                    object,
                });
            }
        }
        if pending.is_none() && document_intent.is_none() {
            if let Some((operations, select_after)) = staged_operations {
                *pending = Some(PendingMutation::VisualTransaction {
                    operations,
                    expected_document: visual_identity,
                    expected_revision: visual_revision,
                    select_after,
                });
            } else if let Some(redo) = stage_history {
                *pending = Some(PendingMutation::VisualHistory {
                    redo,
                    expected_document: visual_identity,
                    expected_revision: visual_revision,
                });
            }
        }
        self.stage_completed_visual_asset_import(
            ui.ctx(),
            structural_edits_allowed,
            closing_requested,
            pending,
            document_intent.is_some(),
        );
        effects.menu_focus_request = menu_focus_request_for_ui(
            ui, effects.active_menu_scope.as_ref(), previous_menu_scope.as_ref(), actions_allowed,
            closing_requested, focused_at_start,
        );
        #[cfg(test)]
        if effects.visual_export_scale_requested.is_none() { effects.visual_export_scale_requested = self.test_visual_export_scale_requested.take(); }
        effects.visual_selection = Some(selection_state);
        effects.visual_canvas_editing_allowed = canvas_interaction_authorized;
        effects.visual_structural_editing_allowed = structural_edits_allowed;
        effects.visual_asset_picker_requested = visual_asset_picker_requested;
        effects
    }
}

fn paint_visual_projection(
    painter: &egui::Painter,
    projection: &VisualSceneProjection,
    canvas: egui::Rect,
    camera: Camera,
    selected_objects: &BTreeSet<VisualObjectId>,
    asset_textures: &HashMap<VisualAssetId, VisualAssetTexture>,
    asset_errors: &HashMap<VisualAssetId, String>,
) -> VisualPaintReport {
    const MAX_PAINT_WORK_UNITS: usize = 65_536;
    let canvas_color = egui::Color32::from_gray(249);
    painter.rect_filled(canvas, 4.0, canvas_color);
    let Some(_bounds) = projection.bounds else {
        painter.text(
            canvas.center(),
            egui::Align2::CENTER_CENTER,
            "Empty visual scene",
            egui::FontId::proportional(14.0),
            egui::Color32::DARK_GRAY,
        );
        return VisualPaintReport::default();
    };
    let available = canvas.shrink(12.0);
    let mapping = VisualCanvasMapping {
        canvas: available,
        camera,
    };
    let mut remaining_work = MAX_PAINT_WORK_UNITS;
    let mut report = VisualPaintReport::default();

    for (item_index, item) in projection.items.iter().enumerate() {
        let item_work = visual_item_paint_work(item);
        if item_work > remaining_work {
            report.omitted_items = projection.items.len() - item_index;
            break;
        }
        remaining_work -= item_work;
        report.painted_items += 1;
        report.visible_item_prefix = item_index + 1;
        if !mapping.rect(item.bounds).intersects(canvas) {
            continue;
        }
        let is_text = item.kind == VisualProjectedKind::Text;
        let is_connector = item.kind == VisualProjectedKind::Connector;
        let is_image = item.kind == VisualProjectedKind::Image;
        let is_frame = item.header.is_some();
        let outline = item
            .outline
            .iter()
            .map(|point| mapping.point(*point))
            .collect::<Vec<_>>();
        if outline.len() < 3 && !is_connector {
            continue;
        }

        if !is_connector && let Some(shadow) = item.style.shadow {
            let shadow_color = visual_color(shadow.color, item.style.opacity);
            let offset = egui::vec2(
                (shadow.offset_x * camera.zoom as f64) as f32,
                (shadow.offset_y * camera.zoom as f64) as f32,
            );
            let shadow_outline = outline
                .iter()
                .map(|point| *point + offset)
                .collect::<Vec<_>>();
            if !is_text {
                let fill = item
                    .header
                    .as_ref()
                    .map(|header| {
                        header
                            .iter()
                            .map(|point| mapping.point(*point) + offset)
                            .collect()
                    })
                    .unwrap_or_else(|| shadow_outline.clone());
                painter.add(egui::Shape::convex_polygon(
                    fill,
                    shadow_color,
                    egui::Stroke::NONE,
                ));
            }
            let shadow_width = (shadow.blur * 2.0 * f64::from(camera.zoom)) as f32;
            if shadow_width > 0.0 {
                paint_solid_path(
                    painter,
                    &shadow_outline,
                    true,
                    egui::Stroke::new(shadow_width, shadow_color),
                );
                if is_frame {
                    for path in &item.details {
                        let points = path
                            .iter()
                            .map(|point| mapping.point(*point) + offset)
                            .collect::<Vec<_>>();
                        paint_solid_path(
                            painter,
                            &points,
                            false,
                            egui::Stroke::new(shadow_width, shadow_color),
                        );
                    }
                }
            }
        }

        if is_connector {
            let style = if item.dash_simplified || item.dash_paths.is_none() {
                VisualLineStyle::Solid
            } else {
                item.style.line_style
            };
            let color = if item.route_fallback {
                visual_color(
                    VisualColor::rgba(196, 91, 42, item.style.border.alpha),
                    item.style.opacity,
                )
            } else {
                visual_color(item.style.border, item.style.opacity)
            };
            let stroke = egui::Stroke::new(
                (item.style.border_width * f64::from(camera.zoom)).max(0.0) as f32,
                color,
            );
            for (index, path) in item.open_paths.iter().enumerate() {
                let mapped_path = path
                    .iter()
                    .map(|point| mapping.point(*point))
                    .collect::<Vec<_>>();
                paint_styled_path(
                    painter,
                    &mapped_path,
                    false,
                    stroke,
                    style,
                    item.dash_paths
                        .as_deref()
                        .and_then(|paths| paths.get(index))
                        .map(Vec::as_slice),
                    &mapping,
                );
            }
        } else if is_image {
            let texture = item.image_asset.as_ref().and_then(|asset| {
                asset_textures
                    .get(asset.id())
                    .filter(|texture| (texture.width, texture.height) == asset.dimensions())
            });
            if let Some(texture) = texture.filter(|_| outline.len() == 4) {
                let mut mesh = egui::epaint::Mesh::with_texture(texture.texture.id());
                let alpha = (item.style.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
                let uv = [
                    egui::pos2(0.0, 0.0),
                    egui::pos2(1.0, 0.0),
                    egui::pos2(1.0, 1.0),
                    egui::pos2(0.0, 1.0),
                ];
                for (position, uv) in outline.iter().copied().zip(uv) {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: position,
                        uv,
                        color: egui::Color32::from_white_alpha(alpha),
                    });
                }
                mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
                painter.add(egui::Shape::mesh(mesh));
            } else {
                painter.add(egui::Shape::convex_polygon(
                    outline.clone(),
                    visual_color(item.style.fill, item.style.opacity),
                    egui::Stroke::NONE,
                ));
                let bounds = egui::Rect::from_points(&outline);
                if bounds.width() >= 76.0 && bounds.height() >= 24.0 {
                    let failed = item
                        .image_asset
                        .as_ref()
                        .is_some_and(|asset| asset_errors.contains_key(asset.id()));
                    painter.text(
                        bounds.center(),
                        egui::Align2::CENTER_CENTER,
                        if failed {
                            "Image unavailable"
                        } else {
                            "Image loading"
                        },
                        egui::FontId::proportional(12.0),
                        egui::Color32::DARK_GRAY,
                    );
                }
            }
            let style = if item.dash_simplified || item.dash_paths.is_none() {
                VisualLineStyle::Solid
            } else {
                item.style.line_style
            };
            let stroke = egui::Stroke::new(
                (item.style.border_width * f64::from(camera.zoom)).max(0.0) as f32,
                visual_color(item.style.border, item.style.opacity),
            );
            paint_styled_path(
                painter,
                &outline,
                true,
                stroke,
                style,
                item.dash_paths
                    .as_deref()
                    .and_then(|paths| paths.first())
                    .map(Vec::as_slice),
                &mapping,
            );
        } else if !is_text {
            let fill = item
                .header
                .as_ref()
                .map(|header| header.iter().map(|point| mapping.point(*point)).collect())
                .unwrap_or_else(|| outline.clone());
            painter.add(egui::Shape::convex_polygon(
                fill,
                visual_color(item.style.fill, item.style.opacity),
                egui::Stroke::NONE,
            ));
            let style = if item.dash_simplified || item.dash_paths.is_none() {
                VisualLineStyle::Solid
            } else {
                item.style.line_style
            };
            let stroke = egui::Stroke::new(
                (item.style.border_width * f64::from(camera.zoom)).max(0.0) as f32,
                visual_color(item.style.border, item.style.opacity),
            );
            let mut mapped_details = Vec::with_capacity(item.details.len());
            for path in &item.details {
                mapped_details.push(
                    path.iter()
                        .map(|point| mapping.point(*point))
                        .collect::<Vec<_>>(),
                );
            }
            let dash_paths = item.dash_paths.as_deref();
            paint_styled_path(
                painter,
                &outline,
                true,
                stroke,
                style,
                dash_paths
                    .and_then(|paths| paths.first())
                    .map(Vec::as_slice),
                &mapping,
            );
            for (index, detail) in mapped_details.iter().enumerate() {
                let details_closed = matches!(
                    item.kind,
                    VisualProjectedKind::Primitive(VisualPrimitiveShape::Cylinder)
                        | VisualProjectedKind::PresetPrimitive(VisualPrimitiveShape::Cylinder)
                );
                paint_styled_path(
                    painter,
                    detail,
                    details_closed,
                    stroke,
                    style,
                    dash_paths
                        .and_then(|paths| paths.get(index + 1))
                        .map(Vec::as_slice),
                    &mapping,
                );
            }
        }

        if let Some(text) = item.text.as_ref() {
            report.suppressed_text_lines += paint_visual_text(painter, &mapping, item, text);
        }

        if selected_objects.contains(&item.object_id) {
            let selected_bounds = mapping.rect(item.bounds).shrink(1.0);
            painter.rect_stroke(
                selected_bounds,
                2.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 112, 201)),
                egui::StrokeKind::Inside,
            );
        }
    }
    report
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct VisualPaintReport {
    painted_items: usize,
    omitted_items: usize,
    suppressed_text_lines: usize,
    visible_item_prefix: usize,
}

fn visual_item_paint_work(item: &crate::diagram::ProjectedVisualItem) -> usize {
    let path_points = item
        .outline
        .len()
        .saturating_add(item.header.as_ref().map_or(0, Vec::len))
        .saturating_add(item.details.iter().map(Vec::len).sum::<usize>())
        .saturating_add(item.open_paths.iter().map(Vec::len).sum::<usize>());
    let text_work = item.text.as_ref().map_or(0, |text| {
        text.lines
            .iter()
            .map(|line| line.scalar_count.saturating_mul(2).saturating_add(8))
            .sum::<usize>()
    });
    let patterned_strokes = item
        .dash_paths
        .as_ref()
        .map_or(0, |paths| paths.iter().map(Vec::len).sum::<usize>());
    path_points
        .saturating_mul(4)
        .saturating_add(text_work)
        .saturating_add(patterned_strokes.saturating_mul(3))
        .saturating_add(32)
}

#[derive(Clone, Copy)]
struct VisualCanvasMapping {
    canvas: egui::Rect,
    camera: Camera,
}

impl VisualCanvasMapping {
    fn point(self, point: ScenePoint) -> egui::Pos2 {
        world_to_screen(point, self.canvas, self.camera)
    }

    fn rect(self, rect: SceneRect) -> egui::Rect {
        world_rect_to_screen(rect, self.canvas, self.camera)
    }
}

fn pure_canvas_mapping(canvas: egui::Rect, camera: Camera) -> CanvasMapping {
    let viewport = canvas_rect_to_scene_rect(canvas);
    CanvasMapping {
        viewport,
        clip: viewport,
        center: camera.center,
        zoom: f64::from(camera.zoom),
    }
}

fn canvas_rect_to_scene_rect(rect: egui::Rect) -> SceneRect {
    SceneRect {
        min: ScenePoint {
            x: f64::from(rect.left()),
            y: f64::from(rect.top()),
        },
        max: ScenePoint {
            x: f64::from(rect.right()),
            y: f64::from(rect.bottom()),
        },
    }
}

fn scene_point_from_screen(point: egui::Pos2) -> ScenePoint {
    ScenePoint {
        x: f64::from(point.x),
        y: f64::from(point.y),
    }
}

fn canvas_mapping_rect(mapping: CanvasMapping) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(mapping.clip.min.x as f32, mapping.clip.min.y as f32),
        egui::pos2(mapping.clip.max.x as f32, mapping.clip.max.y as f32),
    )
}

fn visual_minimap_screen_rect(canvas: egui::Rect, visible_clip: egui::Rect) -> Option<egui::Rect> {
    let clip = visible_clip.intersect(canvas);
    let width = (clip.width() - 12.0).min(136.0);
    let height = (clip.height() - 12.0).min(92.0);
    if width < 48.0 || height < 40.0 {
        return None;
    }
    Some(egui::Rect::from_min_size(
        egui::pos2(clip.right() - width - 6.0, clip.bottom() - height - 6.0),
        egui::vec2(width, height),
    ))
}

fn minimap_layout_for_camera(
    projection: &VisualSceneProjection,
    canvas: egui::Rect,
    visible_clip: egui::Rect,
    frame: egui::Rect,
    camera: Camera,
) -> Option<CanvasMinimap> {
    let mut mapping = pure_canvas_mapping(canvas, camera);
    mapping.clip = canvas_rect_to_scene_rect(visible_clip.intersect(canvas));
    let first = mapping.screen_to_world(mapping.clip.min);
    let second = mapping.screen_to_world(mapping.clip.max);
    let visible_world = SceneRect {
        min: ScenePoint {
            x: first.x.min(second.x),
            y: first.y.min(second.y),
        },
        max: ScenePoint {
            x: first.x.max(second.x),
            y: first.y.max(second.y),
        },
    };
    visual_canvas_minimap_layout(projection, visible_world, canvas_rect_to_scene_rect(frame))
}

#[allow(clippy::too_many_arguments)]
fn handle_visual_minimap_drag(
    ui: &egui::Ui,
    frame: Option<egui::Rect>,
    camera: &mut Camera,
    state: &mut Option<MinimapDrag>,
    actions_allowed: bool,
    host: HostIdentity,
    generation: u64,
    layout: impl FnOnce() -> Option<CanvasMinimap>,
) -> bool {
    if !actions_allowed {
        if !ui.ctx().will_discard() && !ui.is_sizing_pass() {
            cancel_minimap_drag(state, camera);
        }
        return false;
    }
    let (pointer, pressed, down, released, escape) = ui.ctx().input(|input| {
        (
            input.pointer.interact_pos(),
            input.pointer.button_pressed(egui::PointerButton::Primary),
            input.pointer.button_down(egui::PointerButton::Primary),
            input.pointer.button_released(egui::PointerButton::Primary),
            input.key_pressed(egui::Key::Escape),
        )
    });
    if escape {
        cancel_minimap_drag(state, camera);
        return false;
    }
    if pressed
        && pointer.is_some_and(|p| frame.is_some_and(|rect| rect.contains(p)))
        && let (Some(pointer), Some(layout)) = (pointer, layout())
        && let Some(world) = layout.screen_to_world(scene_point_from_screen(pointer))
    {
        let offset = if layout
            .viewport
            .contains_point(scene_point_from_screen(pointer))
        {
            ScenePoint {
                x: camera.center.x - world.x,
                y: camera.center.y - world.y,
            }
        } else {
            ScenePoint::default()
        };
        *state = Some(MinimapDrag {
            layout,
            offset,
            host,
            generation,
            start_pass: ViewportPassProvenance::current(ui.ctx(), ui.ctx().viewport_id()),
            accepted: false,
            released: false,
            last_camera: *camera,
        });
    }
    let active = state.is_some();
    if let Some(drag) = state.as_ref()
        && (down || released)
        && let Some(pointer) = pointer
    {
        let frame = drag.layout.frame;
        let point = ScenePoint {
            x: f64::from(pointer.x).clamp(frame.min.x, frame.max.x),
            y: f64::from(pointer.y).clamp(frame.min.y, frame.max.y),
        };
        if let Some(world) = drag.layout.screen_to_world(point) {
            camera.center = ScenePoint {
                x: world.x + drag.offset.x,
                y: world.y + drag.offset.y,
            };
            camera.fit_next = false;
        }
    }
    if let Some(drag) = state.as_mut() {
        drag.released = released || !down;
    }
    active
}

fn cancel_minimap_drag(state: &mut Option<MinimapDrag>, camera: &mut Camera) {
    if let Some(drag) = state.take() {
        *camera = drag.last_camera;
    }
}

fn finish_minimap_drag(
    state: &mut Option<MinimapDrag>,
    camera: &mut Camera,
    valid: bool,
    host: HostIdentity,
    generation: u64,
    ctx: &egui::Context,
) -> bool {
    let Some(drag) = state.as_mut() else {
        return false;
    };
    if !valid
        || drag.host != host
        || drag.generation != generation
        || (!drag.accepted && !drag.start_pass.was_retained(ctx))
    {
        cancel_minimap_drag(state, camera);
        return false;
    }
    drag.accepted = true;
    drag.last_camera = *camera;
    if drag.released {
        *state = None;
    }
    true
}

fn paint_visual_minimap(painter: &egui::Painter, frame: egui::Rect, minimap: &CanvasMinimap) {
    let frame_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(105));
    painter.rect_filled(frame, 3.0, egui::Color32::from_gray(250));
    for outline in &minimap.outlines {
        if outline.len() < 3 {
            continue;
        }
        let points = outline
            .iter()
            .map(|point| egui::pos2(point.x as f32, point.y as f32))
            .collect::<Vec<_>>();
        painter.add(egui::Shape::closed_line(
            points,
            egui::Stroke::new(0.8, egui::Color32::from_gray(155)),
        ));
    }
    for path in &minimap.open_paths {
        let points = path
            .iter()
            .map(|point| egui::pos2(point.x as f32, point.y as f32))
            .collect::<Vec<_>>();
        for segment in points.windows(2) {
            painter.line_segment(
                [segment[0], segment[1]],
                egui::Stroke::new(0.8, egui::Color32::from_gray(120)),
            );
        }
    }
    let viewport = egui::Rect::from_min_max(
        egui::pos2(minimap.viewport.min.x as f32, minimap.viewport.min.y as f32),
        egui::pos2(minimap.viewport.max.x as f32, minimap.viewport.max.y as f32),
    );
    painter.rect_filled(
        viewport,
        0.0,
        egui::Color32::from_rgba_unmultiplied(36, 112, 201, 35),
    );
    painter.rect_stroke(
        viewport,
        0.0,
        egui::Stroke::new(1.3, egui::Color32::from_rgb(36, 112, 201)),
        egui::StrokeKind::Inside,
    );
    painter.rect_stroke(frame, 3.0, frame_stroke, egui::StrokeKind::Inside);
}

fn paint_visual_canvas_guides(
    painter: &egui::Painter,
    guides: &[CanvasGuide],
    mapping: CanvasMapping,
) {
    let clip = canvas_mapping_rect(mapping).intersect(painter.clip_rect());
    let color = egui::Color32::from_rgb(224, 76, 57);
    for guide in guides {
        let (start, end) = match guide.axis {
            CanvasGuideAxis::Vertical => {
                let x = mapping
                    .world_to_screen(ScenePoint {
                        x: guide.coordinate,
                        y: 0.0,
                    })
                    .x as f32;
                (egui::pos2(x, clip.top()), egui::pos2(x, clip.bottom()))
            }
            CanvasGuideAxis::Horizontal => {
                let y = mapping
                    .world_to_screen(ScenePoint {
                        x: 0.0,
                        y: guide.coordinate,
                    })
                    .y as f32;
                (egui::pos2(clip.left(), y), egui::pos2(clip.right(), y))
            }
        };
        painter.line_segment([start, end], egui::Stroke::new(1.0, color));
    }
}

fn clamp_canvas_screen_point(mapping: CanvasMapping, point: egui::Pos2) -> egui::Pos2 {
    let viewport = canvas_mapping_rect(mapping);
    egui::pos2(
        point.x.clamp(viewport.left(), viewport.right()),
        point.y.clamp(viewport.top(), viewport.bottom()),
    )
}

fn reduce_canvas_selection(
    current: &VisualSelectionState,
    hit: CanvasHit,
    extend: bool,
    retain_selected_member_for_drag: bool,
) -> VisualSelectionState {
    if retain_selected_member_for_drag
        && let CanvasHit::Object(id) = hit
        && current.objects.contains(&id)
        && !extend
    {
        return current.clone();
    }
    let mut next = current.clone();
    match hit {
        CanvasHit::Object(id) if extend => {
            if !next.objects.remove(&id) {
                next.objects.insert(id);
                next.primary = Some(id);
            } else if next.primary == Some(id) {
                next.primary = next.objects.last().copied();
            }
        }
        CanvasHit::Object(id) => {
            next.objects.clear();
            next.objects.insert(id);
            next.primary = Some(id);
        }
        CanvasHit::Empty if !extend => {
            next.objects.clear();
            next.primary = None;
        }
        CanvasHit::Empty | CanvasHit::Handle(_) => {}
    }
    next
}

fn canvas_gesture_mode(
    scene: &VisualScene,
    hit: CanvasHit,
    selection: &VisualSelectionState,
    projection: &VisualSceneProjection,
    painted_item_prefix: usize,
    extend: bool,
) -> Option<VisualGestureMode> {
    let transformable_selection =
        crate::diagram::visual_canvas_resolve_membership(scene, &selection.objects)?
            .expanded
            .into_iter()
            .collect::<BTreeSet<_>>();
    if !transformable_selection.is_empty()
        && matches!(hit, CanvasHit::Handle(_) | CanvasHit::Object(_))
        && visual_canvas_selection_bounds(
            scene,
            projection,
            &selection.objects,
            painted_item_prefix,
        )
        .is_none()
    {
        return None;
    }
    match hit {
        CanvasHit::Handle(CanvasHandleKind::ResizeCorner {
            object_id,
            sign_x,
            sign_y,
        }) if transformable_selection.len() == 1
            && transformable_selection.contains(&object_id) =>
        {
            Some(VisualGestureMode::Transform(
                VisualTransformMode::ResizeCorner { sign_x, sign_y },
            ))
        }
        CanvasHit::Handle(CanvasHandleKind::RotateObject { object_id })
            if transformable_selection.len() == 1
                && transformable_selection.contains(&object_id) =>
        {
            Some(VisualGestureMode::Transform(
                VisualTransformMode::RotateObject,
            ))
        }
        CanvasHit::Handle(CanvasHandleKind::ResizeSelection)
            if transformable_selection.len() > 1 =>
        {
            let pivot = visual_canvas_selection_bounds(
                scene,
                projection,
                &selection.objects,
                painted_item_prefix,
            )?
            .center();
            Some(VisualGestureMode::Transform(
                VisualTransformMode::ResizeSelection { pivot },
            ))
        }
        CanvasHit::Handle(CanvasHandleKind::RotateSelection)
            if transformable_selection.len() > 1 =>
        {
            let pivot = visual_canvas_selection_bounds(
                scene,
                projection,
                &selection.objects,
                painted_item_prefix,
            )?
            .center();
            Some(VisualGestureMode::Transform(
                VisualTransformMode::RotateSelection { pivot },
            ))
        }
        CanvasHit::Object(id)
            if selection.objects.contains(&id)
                && !projection.items.iter().any(|item| {
                    item.object_id == id && item.kind == VisualProjectedKind::Connector
                }) =>
        {
            Some(VisualGestureMode::Transform(VisualTransformMode::Translate))
        }
        CanvasHit::Object(_) => None,
        CanvasHit::Empty => Some(VisualGestureMode::Marquee { extend }),
        _ => None,
    }
}

fn paint_visual_canvas_handles(
    painter: &egui::Painter,
    handles: &[CanvasHandle],
    mapping: CanvasMapping,
) {
    let fill = egui::Color32::WHITE;
    let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(36, 112, 201));
    for handle in handles {
        let center = mapping.world_to_screen(handle.center);
        let position = egui::pos2(center.x as f32, center.y as f32);
        match handle.kind {
            CanvasHandleKind::ResizeCorner { .. } | CanvasHandleKind::ResizeSelection => {
                painter.rect_filled(
                    egui::Rect::from_center_size(position, egui::vec2(8.0, 8.0)),
                    1.0,
                    fill,
                );
                painter.rect_stroke(
                    egui::Rect::from_center_size(position, egui::vec2(8.0, 8.0)),
                    1.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
            CanvasHandleKind::RotateObject { .. } | CanvasHandleKind::RotateSelection => {
                painter.circle_filled(position, 4.5, fill);
                painter.circle_stroke(position, 4.5, stroke);
            }
        }
    }
}

fn visual_painted_item_prefix(projection: &VisualSceneProjection) -> usize {
    const MAX_PAINT_WORK_UNITS: usize = 65_536;
    let mut remaining = MAX_PAINT_WORK_UNITS;
    let mut visible_prefix = 0;
    for item in &projection.items {
        let work = visual_item_paint_work(item);
        if work > remaining {
            break;
        }
        remaining -= work;
        visible_prefix += 1;
    }
    visible_prefix
}

fn visual_font_family_label(family: &str) -> String {
    if family.eq_ignore_ascii_case("sans-serif") {
        "Sans serif".to_owned()
    } else if family.eq_ignore_ascii_case("monospace") {
        "Monospace".to_owned()
    } else {
        format!("{family} (sans-serif fallback)")
    }
}

fn visual_font_family_picker(ui: &mut egui::Ui, family: &mut String) -> egui::Response {
    egui::ComboBox::from_id_salt(ui.next_auto_id())
        .selected_text(visual_font_family_label(family))
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(family.eq_ignore_ascii_case("sans-serif"), "Sans serif")
                .clicked()
            {
                *family = "sans-serif".to_owned();
            }
            let mono = ui.selectable_label(family.eq_ignore_ascii_case("monospace"), "Monospace");
            #[cfg(test)]
            ui.ctx().data_mut(|data| {
                data.insert_temp(egui::Id::new("test-visual-font-monospace"), mono.rect)
            });
            if mono.clicked() {
                *family = "monospace".to_owned();
            }
        })
        .response
}

fn handle_visual_canvas_camera_input(
    ui: &mut egui::Ui,
    response: &egui::Response,
    visible_canvas: egui::Rect,
    excluded_rect: Option<egui::Rect>,
    camera: &mut Camera,
    actions_allowed: bool,
) -> bool {
    let mut navigated = false;
    let pointer = ui.ctx().input(|input| input.pointer.interact_pos());
    if actions_allowed
        && pointer.is_some_and(|pointer| {
            visible_canvas.contains(pointer)
                && !excluded_rect.is_some_and(|excluded| excluded.contains(pointer))
        })
    {
        let scroll = ui.ctx().input_mut(|input| {
            let scroll = input.smooth_scroll_delta.y;
            if scroll.abs() > 0.01 {
                input.smooth_scroll_delta = egui::Vec2::ZERO;
            }
            scroll
        });
        if scroll.abs() > 0.01 {
            navigated = true;
            camera.zoom =
                (camera.zoom * (scroll * 0.001).exp()).clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
            camera.fit_next = false;
        }
    }
    let middle_press_started_visible = ui.ctx().input(|input| {
        input.pointer.press_origin().is_some_and(|origin| {
            visible_canvas.contains(origin)
                && !excluded_rect.is_some_and(|excluded| excluded.contains(origin))
        })
    });
    if actions_allowed
        && middle_press_started_visible
        && response.dragged_by(egui::PointerButton::Middle)
    {
        navigated = true;
        let delta = response.drag_delta();
        camera.center.x -= f64::from(delta.x / camera.zoom);
        camera.center.y -= f64::from(delta.y / camera.zoom);
        camera.fit_next = false;
    }
    navigated
}

fn paint_styled_path(
    painter: &egui::Painter,
    points: &[egui::Pos2],
    closed: bool,
    stroke: egui::Stroke,
    style: VisualLineStyle,
    pattern: Option<&[ProjectedStrokePrimitive]>,
    mapping: &VisualCanvasMapping,
) {
    if stroke.width <= 0.0 || points.len() < 2 {
        return;
    }
    if style != VisualLineStyle::Solid
        && let Some(pattern) = pattern
    {
        for primitive in pattern {
            match *primitive {
                ProjectedStrokePrimitive::Segment { start, end } => {
                    let start = mapping.point(start);
                    let end = mapping.point(end);
                    painter.line_segment([start, end], stroke);
                    painter.circle_filled(start, stroke.width / 2.0, stroke.color);
                    painter.circle_filled(end, stroke.width / 2.0, stroke.color);
                }
                ProjectedStrokePrimitive::Dot { center } => {
                    let center = mapping.point(center);
                    painter.circle_filled(center, stroke.width / 2.0, stroke.color);
                }
            }
        }
        return;
    }
    paint_solid_path(painter, points, closed, stroke);
}

fn paint_solid_path(
    painter: &egui::Painter,
    points: &[egui::Pos2],
    closed: bool,
    stroke: egui::Stroke,
) {
    if points.len() < 2 || stroke.width <= 0.0 {
        return;
    }
    let edge_count = points.len() - 1 + usize::from(closed);
    for edge in 0..edge_count {
        let start = points[edge];
        let end = points[(edge + 1) % points.len()];
        painter.line_segment([start, end], stroke);
    }
    let radius = stroke.width / 2.0;
    for point in points {
        painter.circle_filled(*point, radius, stroke.color);
    }
}

fn paint_visual_text(
    painter: &egui::Painter,
    mapping: &VisualCanvasMapping,
    item: &crate::diagram::ProjectedVisualItem,
    text: &PreparedText,
) -> usize {
    if text.lines.is_empty() {
        return 0;
    }
    let text_transform = item.text_transform.unwrap_or(item.transform);
    let center = ScenePoint {
        x: text_transform.x,
        y: text_transform.y,
    };
    let line_count = text.lines.len() as f64;
    let scale = f64::from(mapping.camera.zoom);
    let line_height = (text.line_height.min(text.height / line_count) * scale) as f32;
    if !line_height.is_finite() || line_height <= 0.0 {
        return text.lines.len();
    }
    let total_height = line_height * text.lines.len() as f32;
    let font_family = if item.style.font_family.eq_ignore_ascii_case("monospace") {
        egui::FontFamily::Monospace
    } else {
        // Unknown persisted font names deliberately fall back to the platform's
        // proportional family; document opening never depends on local fonts.
        egui::FontFamily::Proportional
    };
    let text_color = visual_color(item.style.border, item.style.opacity);
    let font_size = ((item.style.font_size * scale) as f32).min(line_height * 0.8);
    let angle = item.text_rotation_degrees.to_radians() as f32;
    let (sin, cos) = angle.sin_cos();
    let screen_center = mapping.point(center);
    let shadow = (item.kind != VisualProjectedKind::Connector)
        .then_some(item.style.shadow)
        .flatten()
        .map(|shadow| {
            (
                egui::vec2(
                    (shadow.offset_x * scale) as f32,
                    (shadow.offset_y * scale) as f32,
                ),
                visual_color(shadow.color, item.style.opacity),
            )
        });

    let mut suppressed = 0;
    for (index, line) in text.lines.iter().enumerate() {
        if line.text.is_empty() {
            continue;
        }
        let max_width = (line.reserved_width * scale) as f32;
        if !max_width.is_finite() || max_width < 0.75 || font_size < 0.75 {
            suppressed += 1;
            continue;
        }
        let mut fitted_size = font_size;
        let mut galley = None;
        for _ in 0..4 {
            let candidate = painter.layout_no_wrap(
                line.text.clone(),
                egui::FontId::new(fitted_size, font_family.clone()),
                text_color,
            );
            let Some(fit) = visual_galley_ink_fit(&candidate, max_width, line_height) else {
                break;
            };
            if fit >= 1.0 {
                galley = Some(candidate);
                break;
            }
            fitted_size *= fit * 0.99;
            if fitted_size < 0.75 {
                break;
            }
        }
        let Some(galley) = galley else {
            suppressed += 1;
            continue;
        };
        let local_y = -total_height / 2.0 + line_height * (index as f32 + 0.5);
        let rotated_y = local_y * cos;
        let rotated_x = -local_y * sin;
        let line_center = screen_center + egui::vec2(rotated_x, rotated_y);
        let top_left = line_center - galley.rect.center().to_vec2();
        if let Some((offset, shadow_color)) = shadow {
            let shadow_shape =
                egui::epaint::TextShape::new(top_left + offset, galley.clone(), shadow_color)
                    .with_override_text_color(shadow_color)
                    .with_angle_and_anchor(angle, egui::Align2::CENTER_CENTER);
            painter.add(shadow_shape);
        }
        let shape = egui::epaint::TextShape::new(top_left, galley, text_color)
            .with_override_text_color(text_color)
            .with_angle_and_anchor(angle, egui::Align2::CENTER_CENTER);
        painter.add(shape);
    }
    suppressed
}

fn visual_galley_ink_fit(galley: &egui::Galley, max_width: f32, max_height: f32) -> Option<f32> {
    let bounds = galley.mesh_bounds;
    let center = galley.rect.center();
    let coordinates = [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y];
    if coordinates.iter().any(|coordinate| !coordinate.is_finite()) {
        return None;
    }
    let left = (center.x - bounds.min.x).max(0.0);
    let right = (bounds.max.x - center.x).max(0.0);
    let top = (center.y - bounds.min.y).max(0.0);
    let bottom = (bounds.max.y - center.y).max(0.0);
    let ink_width = left.max(right);
    let ink_height = top.max(bottom);
    if ink_width <= 0.0 || ink_height <= 0.0 {
        return None;
    }
    let width_fit = (max_width / 2.0) / ink_width;
    let height_fit = (max_height / 2.0) / ink_height;
    let fit = width_fit.min(height_fit).min(1.0);
    fit.is_finite().then_some(fit.max(0.0))
}

fn visual_color(color: VisualColor, opacity: f64) -> egui::Color32 {
    let [red, green, blue, alpha] = color_with_opacity(color, opacity);
    egui::Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

fn edit_visual_color(ui: &mut egui::Ui, color: &mut VisualColor) -> egui::Response {
    let mut selected = [color.red, color.green, color.blue, color.alpha];
    let response = ui.color_edit_button_srgba_unmultiplied(&mut selected);
    if response.changed() {
        *color = VisualColor::rgba(selected[0], selected[1], selected[2], selected[3]);
    }
    response
}

fn visual_primitive_shapes() -> [VisualPrimitiveShape; 8] {
    [
        VisualPrimitiveShape::Rectangle,
        VisualPrimitiveShape::RoundRect,
        VisualPrimitiveShape::Diamond,
        VisualPrimitiveShape::Circle,
        VisualPrimitiveShape::Ellipse,
        VisualPrimitiveShape::Cylinder,
        VisualPrimitiveShape::Parallelogram,
        VisualPrimitiveShape::Triangle,
    ]
}

const fn visual_shape_label(shape: VisualPrimitiveShape) -> &'static str {
    match shape {
        VisualPrimitiveShape::Rectangle => "Rectangle",
        VisualPrimitiveShape::RoundRect => "Rounded rectangle",
        VisualPrimitiveShape::Diamond => "Diamond",
        VisualPrimitiveShape::Circle => "Circle",
        VisualPrimitiveShape::Ellipse => "Ellipse",
        VisualPrimitiveShape::Cylinder => "Cylinder",
        VisualPrimitiveShape::Parallelogram => "Parallelogram",
        VisualPrimitiveShape::Triangle => "Triangle",
    }
}

fn visual_object_label(object: &VisualObject) -> String {
    let label = match &object.kind {
        VisualObjectKind::Primitive { shape } => visual_shape_label(*shape),
        VisualObjectKind::Text { text } if object.id == crate::diagram::TITLE_OBJECT_ID => {
            return format!("Title — {}", text.chars().take(32).collect::<String>());
        }
        VisualObjectKind::Text { .. } => "Text",
        VisualObjectKind::PresetInstance { .. } => "Preset instance",
        VisualObjectKind::ImageReference { .. } => "Image reference",
        VisualObjectKind::Connector { .. } => "Connector",
        VisualObjectKind::Container { label } => {
            return format!(
                "Container — {} #{}",
                label.chars().take(24).collect::<String>(),
                object.id.get()
            );
        }
        VisualObjectKind::Swimlane { orientation, label } => {
            let orientation = match orientation {
                VisualSwimlaneOrientation::Horizontal => "Horizontal lane",
                VisualSwimlaneOrientation::Vertical => "Vertical lane",
            };
            return format!(
                "{orientation} — {} #{}",
                label.chars().take(24).collect::<String>(),
                object.id.get()
            );
        }
    };
    format!("{label} #{}", object.id.get())
}

fn visual_group_roots(
    scene: &VisualScene,
    selection: &BTreeSet<VisualObjectId>,
) -> Vec<VisualObjectId> {
    crate::diagram::visual_canvas_resolve_membership(scene, selection).map_or_else(
        Vec::new,
        |membership| {
            membership
                .roots
                .into_iter()
                .filter(|id| *id != crate::diagram::TITLE_OBJECT_ID)
                .collect()
        },
    )
}

fn visual_frame_can_parent(
    scene: &VisualScene,
    child: VisualObjectId,
    parent: VisualObjectId,
) -> bool {
    if child == parent {
        return false;
    }
    if !scene.objects.iter().any(|object| {
        object.id == parent
            && matches!(
                &object.kind,
                VisualObjectKind::Container { .. } | VisualObjectKind::Swimlane { .. }
            )
    }) {
        return false;
    }
    let mut current = Some(parent);
    let mut depth = 0usize;
    while let Some(id) = current {
        if id == child {
            return false;
        }
        depth += 1;
        if depth > crate::diagram::MAX_VISUAL_CONTAINMENT_DEPTH {
            return false;
        }
        current = scene.parent_by_child.get(&id).copied();
    }
    true
}

fn visual_connector_endpoint_editor(
    ui: &mut egui::Ui,
    label: &str,
    id_salt: &str,
    action_name: &str,
    endpoint: &mut VisualConnectorEndpoint,
    connector_id: VisualObjectId,
    objects: &[VisualObject],
    mut record_action: impl FnMut(Option<VisualObjectId>, &str, egui::Rect, egui::Rect),
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        let endpoint_object = objects
            .iter()
            .find(|object| object.id == endpoint.object_id);
        let object_combo = egui::ComboBox::from_id_salt((id_salt, "object"))
            .selected_text(endpoint_object.map_or_else(
                || format!("Missing #{}", endpoint.object_id.get()),
                visual_object_label,
            ))
            .show_ui(ui, |ui| {
                for object in objects.iter().filter(|object| {
                    object.id != connector_id
                        && !matches!(&object.kind, VisualObjectKind::Connector { .. })
                }) {
                    let response = ui.selectable_value(
                        &mut endpoint.object_id,
                        object.id,
                        visual_object_label(object),
                    );
                    record_action(Some(object.id), action_name, response.rect, ui.clip_rect());
                }
            });
        record_action(
            None,
            action_name,
            object_combo.response.rect,
            ui.clip_rect(),
        );
        egui::ComboBox::from_id_salt((id_salt, "anchor"))
            .selected_text(visual_anchor_label(endpoint.anchor))
            .show_ui(ui, |ui| {
                for anchor in [
                    VisualAnchor::North,
                    VisualAnchor::NorthEast,
                    VisualAnchor::East,
                    VisualAnchor::SouthEast,
                    VisualAnchor::South,
                    VisualAnchor::SouthWest,
                    VisualAnchor::West,
                    VisualAnchor::NorthWest,
                ] {
                    ui.selectable_value(&mut endpoint.anchor, anchor, visual_anchor_label(anchor));
                }
            });
    });
}

const fn visual_anchor_label(anchor: VisualAnchor) -> &'static str {
    match anchor {
        VisualAnchor::North => "North",
        VisualAnchor::NorthEast => "North east",
        VisualAnchor::East => "East",
        VisualAnchor::SouthEast => "South east",
        VisualAnchor::South => "South",
        VisualAnchor::SouthWest => "South west",
        VisualAnchor::West => "West",
        VisualAnchor::NorthWest => "North west",
    }
}

const fn visual_connector_route_label(route: VisualConnectorRoute) -> &'static str {
    match route {
        VisualConnectorRoute::Straight => "Straight",
        VisualConnectorRoute::Elbow => "Elbow",
        VisualConnectorRoute::Curved => "Curved",
    }
}

const fn visual_connector_direction_label(direction: VisualConnectorDirection) -> &'static str {
    match direction {
        VisualConnectorDirection::None => "No arrows",
        VisualConnectorDirection::Forward => "Forward",
        VisualConnectorDirection::Reverse => "Reverse",
        VisualConnectorDirection::Both => "Both ends",
    }
}

fn nearest_visual_anchor(object: &VisualObject, target: ScenePoint) -> VisualAnchor {
    let dx = target.x - object.transform.x;
    let dy = target.y - object.transform.y;
    let angle = -object.transform.rotation_degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let local_x = dx * cos - dy * sin;
    let local_y = dx * sin + dy * cos;
    let octant = ((local_y.atan2(local_x) + std::f64::consts::FRAC_PI_8)
        .rem_euclid(std::f64::consts::TAU)
        / std::f64::consts::FRAC_PI_4)
        .floor() as u8;
    match octant {
        0 => VisualAnchor::East,
        1 => VisualAnchor::SouthEast,
        2 => VisualAnchor::South,
        3 => VisualAnchor::SouthWest,
        4 => VisualAnchor::West,
        5 => VisualAnchor::NorthWest,
        6 => VisualAnchor::North,
        _ => VisualAnchor::NorthEast,
    }
}

fn visual_line_style_label(style: VisualLineStyle) -> &'static str {
    match style {
        VisualLineStyle::Solid => "Solid",
        VisualLineStyle::Dashed => "Dashed",
        VisualLineStyle::Dotted => "Dotted",
    }
}

fn next_visual_object_transform(scene: Option<&VisualScene>) -> VisualTransform {
    let index = scene
        .map_or(1, |scene| scene.objects.len())
        .saturating_sub(1);
    let column = index % 4;
    let row = index / 4;
    VisualTransform {
        x: (column as f64 - 1.5) * 190.0,
        y: row as f64 * 130.0,
        width: 150.0,
        height: 82.0,
        rotation_degrees: 0.0,
    }
}

fn next_visual_frame_transform(scene: Option<&VisualScene>) -> VisualTransform {
    let mut transform = next_visual_object_transform(scene);
    transform.width = 320.0;
    transform.height = 220.0;
    transform
}

fn visual_frame_style() -> VisualStyle {
    VisualStyle {
        fill: VisualColor::rgba(84, 130, 176, 48),
        ..VisualStyle::default()
    }
}

fn next_visual_preset_instance_transform(scene: Option<&VisualScene>) -> VisualTransform {
    let mut transform = next_visual_object_transform(scene);
    let side = transform.width.max(transform.height);
    transform.width = side;
    transform.height = side;
    transform
}

fn generic_preview_output<'a>(
    status: &'a GenericPreviewStatus,
    expected_key: &RenderKey,
) -> Option<&'a std::sync::Arc<DiagramRenderOutput>> {
    match status {
        GenericPreviewStatus::Ready { key, output, .. } if key == expected_key => Some(output),
        _ => None,
    }
}

fn generic_preview_status_text(
    status: &GenericPreviewStatus,
    expected_key: &RenderKey,
    showing_stale: bool,
) -> Option<(egui::Color32, String)> {
    match status {
        GenericPreviewStatus::Ready { key, .. } if key == expected_key => None,
        GenericPreviewStatus::Pending { key, .. } if key == expected_key => Some((
            egui::Color32::from_rgb(190, 104, 18),
            if showing_stale {
                "STALE PREVIEW — rendering the current source; showing the previous render.".into()
            } else {
                "Rendering Mermaid preview…".into()
            },
        )),
        GenericPreviewStatus::Failed { key, error, .. } if key == expected_key => Some((
            egui::Color32::from_rgb(185, 65, 65),
            if showing_stale {
                format!("STALE PREVIEW — {error}; showing the previous render.")
            } else {
                format!("Mermaid preview unavailable: {error}")
            },
        )),
        _ => Some((
            egui::Color32::from_rgb(110, 118, 129),
            if showing_stale {
                "STALE PREVIEW — preparing the current Mermaid render; showing the previous render."
                    .into()
            } else {
                "Preparing Mermaid preview…".into()
            },
        )),
    }
}

fn generic_preview_canvas_placeholder(
    status: &GenericPreviewStatus,
    expected_key: &RenderKey,
) -> &'static str {
    match status {
        GenericPreviewStatus::Pending { key, .. } if key == expected_key => {
            "Rendering Mermaid preview…"
        }
        GenericPreviewStatus::Failed { key, .. } if key == expected_key => {
            "No preview image is available."
        }
        _ => "Preparing Mermaid preview…",
    }
}

fn raster_content_uv(raster: &crate::diagram::RasterImage) -> egui::Rect {
    let mut min_x = raster.width;
    let mut min_y = raster.height;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for (index, pixel) in raster.rgba.as_chunks::<4>().0.iter().enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let x = (index % raster.width as usize) as u32;
        let y = (index / raster.width as usize) as u32;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x.saturating_add(1));
        max_y = max_y.max(y.saturating_add(1));
    }
    if min_x >= max_x || min_y >= max_y {
        return egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    }
    egui::Rect::from_min_max(
        egui::pos2(
            min_x as f32 / raster.width as f32,
            min_y as f32 / raster.height as f32,
        ),
        egui::pos2(
            max_x as f32 / raster.width as f32,
            max_y as f32 / raster.height as f32,
        ),
    )
}

fn source_editor(
    ui: &mut egui::Ui,
    session: &DiagramSession,
    render_document: &RenderDocumentIdentity,
    actions_allowed: bool,
    pending: &mut Option<PendingMutation>,
    focus_source: bool,
    source_focus_requested: &mut Option<egui::Id>,
    #[cfg(test)] test_editor_id: &mut Option<egui::Id>,
    #[cfg(test)] test_editor_rect: &mut Option<egui::Rect>,
    #[cfg(test)] test_editor_enabled: &mut bool,
    #[cfg(test)] test_action_rects: &mut Vec<(String, egui::Rect, egui::Rect)>,
    #[cfg(test)] test_action_focus_ids: &mut Vec<(String, egui::Id)>,
) {
    ui.label("Mermaid source (raw text is retained exactly)");
    let editor_id = ui.make_persistent_id("diagram-studio-source-editor");
    if focus_source {
        *source_focus_requested = Some(editor_id);
    }
    clear_text_edit_undoer(ui.ctx(), editor_id);
    let mut source = session.draft().to_owned();
    let response = ui.add_enabled(
        actions_allowed,
        egui::TextEdit::multiline(&mut source)
            .id(editor_id)
            .desired_width(f32::INFINITY)
            .desired_rows(14)
            .code_editor()
            .lock_focus(false),
    );
    #[cfg(test)]
    {
        *test_editor_id = Some(editor_id);
        *test_editor_rect = Some(response.rect);
        *test_editor_enabled = response.enabled();
    }
    let source_changed = response.changed() && actions_allowed && pending.is_none();
    let mut suggestion_action_index = 0usize;
    let _assistance_menu = ui.menu_button("Suggestions", |menu_ui| {
        let cursor_character =
            egui::widgets::text_edit::TextEditState::load(menu_ui.ctx(), editor_id)
                .and_then(|state| state.cursor.char_range())
                .map_or(source.chars().count(), |range| range.primary.index.into());
        let cursor_byte = char_index_to_byte(&source, cursor_character);
        if let Some(assistance) = crate::diagram::mermaid_assistance_at(&source, cursor_byte) {
            let context_label = match assistance.context {
                crate::diagram::MermaidAssistanceContext::Header => "Supported flowchart headers",
                crate::diagram::MermaidAssistanceContext::Direction => "Flowchart direction",
                crate::diagram::MermaidAssistanceContext::Statement => "Supported statements",
                crate::diagram::MermaidAssistanceContext::EdgeTarget => "Existing node IDs",
            };
            menu_ui.weak(context_label);
            for suggestion in &assistance.suggestions {
                let index = suggestion_action_index;
                suggestion_action_index += 1;
                let _action_name = format!("mermaid_assist_suggestion_{index}");
                let suggestion_response = menu_ui.add_enabled(
                    actions_allowed && pending.is_none() && !source_changed,
                    egui::Button::new(&suggestion.label),
                );
                #[cfg(test)]
                {
                    test_action_rects.push((
                        _action_name.clone(),
                        suggestion_response.rect,
                        menu_ui.clip_rect(),
                    ));
                    test_action_focus_ids.push((_action_name, suggestion_response.id));
                }
                if suggestion_response.clicked()
                    && pending.is_none()
                    && !source_changed
                    && let Some((updated, cursor_byte)) =
                        crate::diagram::apply_mermaid_assistance(&source, suggestion)
                {
                    let cursor_character = updated[..cursor_byte].chars().count();
                    *pending = Some(PendingMutation::ReplaceSourceWithCursor {
                        source: updated,
                        expected_document: render_document.clone(),
                        expected_revision: session.revision(),
                        editor_id,
                        cursor_character,
                    });
                    menu_ui.close();
                }
            }
        } else {
            menu_ui.weak("No supported suggestion at this position.");
        }
    });
    #[cfg(test)]
    {
        test_action_rects.push((
            "mermaid_assist_menu".into(),
            _assistance_menu.response.rect,
            ui.clip_rect(),
        ));
        test_action_focus_ids.push(("mermaid_assist_menu".into(), _assistance_menu.response.id));
    }
    if source_changed && pending.is_none() {
        *pending = Some(PendingMutation::ReplaceSource(source));
    }
    ui.small("Supported syntax is documented in the Diagram Studio help section.");
}

fn clear_text_edit_undoer(ctx: &egui::Context, id: egui::Id) {
    if let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ctx, id) {
        state.clear_undoer();
        state.store(ctx, id);
    }
}

fn install_text_edit_cursor(ctx: &egui::Context, id: egui::Id, character_index: usize) {
    let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(character_index),
        )));
    state.store(ctx, id);
}

fn char_index_to_byte(source: &str, character_index: usize) -> usize {
    source
        .char_indices()
        .nth(character_index)
        .map_or(source.len(), |(byte_index, _)| byte_index)
}

fn direction_control(
    ui: &mut egui::Ui,
    graph: Option<&DiagramDocument>,
    graph_is_current: bool,
    actions_allowed: bool,
    revision: u64,
    pending: &mut Option<PendingMutation>,
) {
    let Some(graph) = graph else {
        ui.add_enabled_ui(false, |ui| {
            ui.label("Direction unavailable");
        });
        return;
    };
    let mut direction = graph.direction;
    ui.add_enabled_ui(actions_allowed && graph_is_current, |ui| {
        egui::ComboBox::from_id_salt("diagram-studio-direction")
            .selected_text(direction.as_mermaid())
            .show_ui(ui, |ui| {
                for option in [
                    FlowDirection::TopDown,
                    FlowDirection::TopBottom,
                    FlowDirection::LeftRight,
                ] {
                    ui.selectable_value(&mut direction, option, option.as_mermaid());
                }
            });
    });
    if direction != graph.direction && pending.is_none() {
        *pending = Some(PendingMutation::Transaction {
            operations: vec![GraphOperation::SetDirection(direction)],
            expected_revision: revision,
            select_after: None,
        });
    }
}

fn validated_split_ratio(
    ratio: f32,
    total_width: f32,
    splitter_width: f32,
    min_first_width: f32,
    min_second_width: f32,
    default_ratio: f32,
) -> f32 {
    let fallback = if default_ratio.is_finite() {
        default_ratio.clamp(0.0, 1.0)
    } else {
        0.5
    };
    if !total_width.is_finite() || !splitter_width.is_finite() || total_width <= splitter_width {
        return fallback;
    }

    let content_width = total_width - splitter_width;
    if !min_first_width.is_finite()
        || !min_second_width.is_finite()
        || min_first_width < 0.0
        || min_second_width < 0.0
        || content_width < min_first_width + min_second_width
    {
        return fallback;
    }

    let minimum_ratio = min_first_width / content_width;
    let maximum_ratio = 1.0 - min_second_width / content_width;
    let ratio = if ratio.is_finite() { ratio } else { fallback };
    ratio.clamp(minimum_ratio, maximum_ratio)
}

fn split_pane_widths(
    total_width: f32,
    ratio: f32,
    min_first_width: f32,
    min_second_width: f32,
    default_ratio: f32,
) -> (f32, f32) {
    let total_width = if total_width.is_finite() {
        total_width.max(0.0)
    } else {
        0.0
    };
    let content_width = (total_width - SPLITTER_WIDTH).max(0.0);
    let ratio = validated_split_ratio(
        ratio,
        total_width,
        SPLITTER_WIDTH,
        min_first_width,
        min_second_width,
        default_ratio,
    );
    (content_width * ratio, content_width * (1.0 - ratio))
}

fn split_ratio_from_pointer(
    pointer_x: f32,
    splitter_left: f32,
    first_width: f32,
    total_width: f32,
) -> f32 {
    let split_left = splitter_left - first_width;
    (pointer_x - split_left) / (total_width - SPLITTER_WIDTH)
}

fn draw_splitter(
    ui: &mut egui::Ui,
    id_salt: &str,
    height: f32,
    actions_allowed: bool,
) -> (egui::Rect, egui::Response) {
    let (_, rect) = ui.allocate_space(egui::vec2(SPLITTER_WIDTH, height.max(0.0)));
    let sense = if actions_allowed {
        egui::Sense::drag()
    } else {
        egui::Sense::hover()
    };
    let response = ui.interact(
        rect,
        ui.make_persistent_id(("diagram-studio-splitter", id_salt)),
        sense,
    );
    let fill = if response.dragged() {
        ui.visuals().widgets.active.bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    ui.painter().rect_filled(rect, 2.0, fill);
    let x = rect.center().x;
    ui.painter().line_segment(
        [
            egui::pos2(x, rect.top() + 4.0),
            egui::pos2(x, rect.bottom() - 4.0),
        ],
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color),
    );
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    (rect, response)
}

#[allow(clippy::too_many_arguments)]
fn preview_workspace(
    ui: &mut egui::Ui,
    generic_source: bool,
    generic_texture: Option<&GenericPreviewTexture>,
    generic_empty_message: &str,
    graph: Option<&DiagramDocument>,
    scene: Option<&DiagramScene>,
    graph_is_current: bool,
    revision: u64,
    camera: &mut Camera,
    inspector_canvas_split_ratio: &mut f32,
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    property_draft: &mut Option<PropertyDraft>,
    drag: &mut Option<DragState>,
    actions_allowed: bool,
    pending: &mut Option<PendingMutation>,
    message: &mut Option<StudioNotice>,
    canvas_focus_requested: &mut Option<egui::Id>,
    #[cfg(test)] test_canvas_rect: &mut Option<egui::Rect>,
    #[cfg(test)] test_canvas_id: &mut Option<egui::Id>,
    #[cfg(test)] test_action_rects: &mut Vec<(String, egui::Rect, egui::Rect)>,
    #[cfg(test)] test_generic_preview_drawn: &mut bool,
    #[cfg(test)] test_native_inspector_visible: &mut bool,
    #[cfg(test)] test_generic_canvas_placeholder: &mut Option<String>,
) {
    if generic_source {
        #[cfg(test)]
        {
            *test_native_inspector_visible = false;
        }
        let _rect = draw_generic_canvas(
            ui,
            generic_texture,
            camera,
            actions_allowed,
            generic_empty_message,
            canvas_focus_requested,
            false,
            #[cfg(test)]
            test_canvas_id,
            #[cfg(test)]
            test_generic_preview_drawn,
            #[cfg(test)]
            test_generic_canvas_placeholder,
        );
        #[cfg(test)]
        {
            *test_canvas_rect = Some(_rect);
        }
    } else {
        #[cfg(test)]
        {
            *test_native_inspector_visible = true;
        }
        visual_workspace(
            ui,
            graph,
            scene,
            graph_is_current,
            revision,
            camera,
            inspector_canvas_split_ratio,
            selection,
            selected_nodes,
            property_draft,
            drag,
            actions_allowed,
            pending,
            message,
            canvas_focus_requested,
            #[cfg(test)]
            test_canvas_rect,
            #[cfg(test)]
            test_canvas_id,
            #[cfg(test)]
            test_action_rects,
        );
    }
}

fn draw_generic_canvas(
    ui: &mut egui::Ui,
    texture: Option<&GenericPreviewTexture>,
    camera: &mut Camera,
    actions_allowed: bool,
    empty_message: &str,
    canvas_focus_requested: &mut Option<egui::Id>,
    keyboard_camera_shortcuts: bool,
    #[cfg(test)] test_canvas_id: &mut Option<egui::Id>,
    #[cfg(test)] test_generic_preview_drawn: &mut bool,
    #[cfg(test)] test_generic_canvas_placeholder: &mut Option<String>,
) -> egui::Rect {
    let width = ui.available_width().max(CANVAS_MIN_WIDTH);
    let height = ui.available_height().max(CANVAS_MIN_HEIGHT);
    let (canvas_response, painter) =
        ui.allocate_painter(egui::vec2(width, height), egui::Sense::click_and_drag());
    let canvas_rect = canvas_response.rect;
    #[cfg(test)]
    {
        *test_canvas_id = Some(canvas_response.id);
    }
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(canvas_rect));
    painter.rect_filled(canvas_rect, 0.0, egui::Color32::from_rgb(248, 249, 251));

    let pointer = ui.ctx().input(|input| input.pointer.interact_pos());
    if actions_allowed && pointer.is_some_and(|pointer| canvas_rect.contains(pointer)) {
        let scroll = ui.ctx().input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > 0.01 {
            camera.zoom =
                (camera.zoom * (scroll * 0.001).exp()).clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
        }
    }
    if actions_allowed && canvas_response.dragged_by(egui::PointerButton::Middle) {
        let delta = canvas_response.drag_delta();
        camera.center.x -= f64::from(delta.x / camera.zoom);
        camera.center.y -= f64::from(delta.y / camera.zoom);
    }
    if keyboard_camera_shortcuts && actions_allowed {
        if canvas_response.clicked() {
            *canvas_focus_requested = Some(canvas_response.id);
        }
        viewer_canvas_shortcuts(ui, canvas_response.id, camera);
    }

    if let Some(texture) = texture {
        let bounds = SceneRect {
            min: ScenePoint::default(),
            max: ScenePoint {
                x: f64::from(texture.natural_width),
                y: f64::from(texture.natural_height),
            },
        };
        if camera.fit_next {
            fit_camera(camera, bounds, canvas_rect);
        }
        let image_rect = world_rect_to_screen(bounds, canvas_rect, *camera);
        painter.image(
            texture.texture.id(),
            image_rect,
            texture.content_uv,
            egui::Color32::WHITE,
        );
        #[cfg(test)]
        {
            *test_generic_preview_drawn = true;
        }
    } else {
        painter.text(
            canvas_rect.center(),
            egui::Align2::CENTER_CENTER,
            empty_message,
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().text_color(),
        );
        #[cfg(test)]
        {
            *test_generic_canvas_placeholder = Some(empty_message.to_owned());
        }
    }
    canvas_rect
}

fn viewer_canvas_shortcuts(ui: &mut egui::Ui, canvas_id: egui::Id, camera: &mut Camera) -> bool {
    if !ui.memory(|memory| memory.has_focus(canvas_id)) {
        return false;
    }
    enum Action {
        Pan(f64, f64),
        ZoomIn,
        ZoomOut,
    }
    let action = ui.input_mut(|input| {
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowLeft) {
            Some(Action::Pan(-1.0, 0.0))
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowRight) {
            Some(Action::Pan(1.0, 0.0))
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowUp) {
            Some(Action::Pan(0.0, -1.0))
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowDown) {
            Some(Action::Pan(0.0, 1.0))
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Plus)
            || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Equals)
        {
            Some(Action::ZoomIn)
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Minus) {
            Some(Action::ZoomOut)
        } else {
            None
        }
    });
    let navigated = action.is_some();
    match action {
        Some(Action::Pan(x, y)) => {
            camera.center.x += x * CAMERA_KEYBOARD_PAN_SCREEN_POINTS / f64::from(camera.zoom);
            camera.center.y += y * CAMERA_KEYBOARD_PAN_SCREEN_POINTS / f64::from(camera.zoom);
            camera.fit_next = false;
        }
        Some(Action::ZoomIn) => {
            camera.zoom =
                (camera.zoom * CAMERA_KEYBOARD_ZOOM_FACTOR).clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
            camera.fit_next = false;
        }
        Some(Action::ZoomOut) => {
            camera.zoom =
                (camera.zoom / CAMERA_KEYBOARD_ZOOM_FACTOR).clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
            camera.fit_next = false;
        }
        None => {}
    }
    navigated
}

#[allow(clippy::too_many_arguments)]
fn visual_workspace(
    ui: &mut egui::Ui,
    graph: Option<&DiagramDocument>,
    scene: Option<&DiagramScene>,
    graph_is_current: bool,
    revision: u64,
    camera: &mut Camera,
    inspector_canvas_split_ratio: &mut f32,
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    property_draft: &mut Option<PropertyDraft>,
    drag: &mut Option<DragState>,
    actions_allowed: bool,
    pending: &mut Option<PendingMutation>,
    message: &mut Option<StudioNotice>,
    canvas_focus_requested: &mut Option<egui::Id>,
    #[cfg(test)] test_canvas_rect: &mut Option<egui::Rect>,
    #[cfg(test)] test_canvas_id: &mut Option<egui::Id>,
    #[cfg(test)] test_action_rects: &mut Vec<(String, egui::Rect, egui::Rect)>,
) {
    if ui.available_width() < VISUAL_TWO_COLUMN_MIN_WIDTH {
        draw_inspector(
            ui,
            graph,
            graph_is_current,
            revision,
            selection,
            selected_nodes,
            property_draft,
            actions_allowed,
            pending,
        );
        ui.separator();
        let _canvas_rect = ui
            .scope_builder(canvas_scope_builder(), |ui| {
                draw_canvas(
                    ui,
                    graph,
                    scene,
                    graph_is_current,
                    revision,
                    camera,
                    selection,
                    selected_nodes,
                    drag,
                    actions_allowed,
                    CanvasInteraction::Editor,
                    pending,
                    message,
                    canvas_focus_requested,
                    #[cfg(test)]
                    test_canvas_id,
                )
            })
            .inner;
        #[cfg(test)]
        {
            *test_canvas_rect = Some(_canvas_rect);
        }
    } else {
        let available_width = ui.available_width();
        let available_height = ui.available_height();
        let (inspector_width, canvas_width) = split_pane_widths(
            available_width,
            *inspector_canvas_split_ratio,
            INSPECTOR_PANE_MIN_WIDTH,
            CANVAS_MIN_WIDTH,
            DEFAULT_INSPECTOR_CANVAS_RATIO,
        );
        *inspector_canvas_split_ratio = validated_split_ratio(
            *inspector_canvas_split_ratio,
            available_width,
            SPLITTER_WIDTH,
            INSPECTOR_PANE_MIN_WIDTH,
            CANVAS_MIN_WIDTH,
            DEFAULT_INSPECTOR_CANVAS_RATIO,
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(
                egui::vec2(inspector_width, available_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    draw_inspector(
                        ui,
                        graph,
                        graph_is_current,
                        revision,
                        selection,
                        selected_nodes,
                        property_draft,
                        actions_allowed,
                        pending,
                    );
                },
            );
            let (splitter_rect, splitter_response) =
                draw_splitter(ui, "inspector-canvas", available_height, actions_allowed);
            #[cfg(test)]
            test_action_rects.push((
                "inspector_canvas_splitter".into(),
                splitter_rect,
                ui.clip_rect(),
            ));
            if actions_allowed
                && splitter_response.dragged_by(egui::PointerButton::Primary)
                && let Some(pointer) = splitter_response.interact_pointer_pos()
            {
                *inspector_canvas_split_ratio = validated_split_ratio(
                    split_ratio_from_pointer(
                        pointer.x,
                        splitter_rect.left(),
                        inspector_width,
                        available_width,
                    ),
                    available_width,
                    SPLITTER_WIDTH,
                    INSPECTOR_PANE_MIN_WIDTH,
                    CANVAS_MIN_WIDTH,
                    DEFAULT_INSPECTOR_CANVAS_RATIO,
                );
            }
            let _canvas_rect = ui
                .allocate_ui_with_layout(
                    egui::vec2(canvas_width, available_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.scope_builder(canvas_scope_builder(), |ui| {
                            draw_canvas(
                                ui,
                                graph,
                                scene,
                                graph_is_current,
                                revision,
                                camera,
                                selection,
                                selected_nodes,
                                drag,
                                actions_allowed,
                                CanvasInteraction::Editor,
                                pending,
                                message,
                                canvas_focus_requested,
                                #[cfg(test)]
                                test_canvas_id,
                            )
                        })
                        .inner
                    },
                )
                .inner;
            #[cfg(test)]
            {
                *test_canvas_rect = Some(_canvas_rect);
            }
        });
    }
}

fn canvas_scope_builder() -> egui::UiBuilder {
    egui::UiBuilder::new()
        .id(egui::Id::new("diagram-studio-canvas-scope"))
        .layout(egui::Layout::top_down(egui::Align::Min))
}

fn viewer_canvas_scope_builder() -> egui::UiBuilder {
    egui::UiBuilder::new()
        .id(egui::Id::new("diagram-studio-viewer-canvas-scope"))
        .layout(egui::Layout::top_down(egui::Align::Min))
}

#[allow(clippy::too_many_arguments)]
fn draw_inspector(
    ui: &mut egui::Ui,
    graph: Option<&DiagramDocument>,
    graph_is_current: bool,
    revision: u64,
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    property_draft: &mut Option<PropertyDraft>,
    actions_allowed: bool,
    pending: &mut Option<PendingMutation>,
) {
    ui.set_min_width(INSPECTOR_PANE_MIN_WIDTH);
    ui.heading("Structure");
    ui.small("Click list entries or nodes to inspect them.");
    if let Some(graph) = graph {
        ui.collapsing(format!("Nodes ({})", graph.nodes.len()), |ui| {
            for node in &graph.nodes {
                let selected = selected_nodes.contains(&node.id);
                let response = ui.selectable_label(
                    selected,
                    format!("{}  {}", node.id, node.label.replace('\n', " ")),
                );
                if actions_allowed && response.clicked() {
                    select_element(
                        selection,
                        selected_nodes,
                        DiagramElement::Node(node.id.clone()),
                        response
                            .ctx
                            .input(|input| input.modifiers.shift || input.modifiers.command),
                    );
                }
            }
        });
        ui.collapsing(format!("Edges ({})", graph.edges.len()), |ui| {
            for edge in &graph.edges {
                let element = DiagramElement::Edge(edge.id);
                let label = edge.label.as_deref().unwrap_or("(unlabeled)");
                let response = ui.selectable_label(
                    selection.as_ref() == Some(&element),
                    format!("{} → {}  {label}", edge.from, edge.to),
                );
                if actions_allowed && response.clicked() {
                    select_element(selection, selected_nodes, element, false);
                }
            }
        });
        ui.collapsing(format!("Groups ({})", graph.groups.len()), |ui| {
            for group in &graph.groups {
                let element = DiagramElement::Group(group.id.clone());
                let response = ui.selectable_label(
                    selection.as_ref() == Some(&element),
                    format!("{}  {}", group.id, group.label),
                );
                if actions_allowed && response.clicked() {
                    select_element(selection, selected_nodes, element, false);
                }
            }
        });
    } else {
        ui.label("No valid graph preview.");
    }
    ui.separator();
    ui.heading("Properties");
    draw_properties(
        ui,
        graph,
        graph_is_current,
        revision,
        selection,
        property_draft,
        actions_allowed,
        pending,
    );
}

fn select_element(
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    element: DiagramElement,
    extend: bool,
) {
    match &element {
        DiagramElement::Node(id) if extend => {
            if !selected_nodes.insert(id.clone()) {
                selected_nodes.remove(id);
            }
            *selection = if selected_nodes.contains(id) {
                Some(element)
            } else {
                selected_nodes.last().cloned().map(DiagramElement::Node)
            };
        }
        DiagramElement::Node(id) => {
            selected_nodes.clear();
            selected_nodes.insert(id.clone());
            *selection = Some(element);
        }
        _ => {
            selected_nodes.clear();
            *selection = Some(element);
        }
    }
}

fn select_canvas_element(
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    element: DiagramElement,
    extend: bool,
    canvas_id: egui::Id,
    canvas_focus_requested: &mut Option<egui::Id>,
) {
    select_element(selection, selected_nodes, element, extend);
    *canvas_focus_requested = Some(canvas_id);
}

fn draw_properties(
    ui: &mut egui::Ui,
    graph: Option<&DiagramDocument>,
    graph_is_current: bool,
    revision: u64,
    selection: &Option<DiagramElement>,
    property_draft: &mut Option<PropertyDraft>,
    actions_allowed: bool,
    pending: &mut Option<PendingMutation>,
) {
    let (Some(graph), Some(selection)) = (graph, selection.as_ref()) else {
        *property_draft = None;
        ui.label("Select a node, edge, or group.");
        return;
    };
    if property_draft
        .as_ref()
        .is_none_or(|draft| draft.target != *selection)
    {
        *property_draft = property_draft_for(graph, selection, revision);
    }
    let stale_before_reload = property_draft
        .as_ref()
        .is_some_and(|draft| draft.basis_revision != revision);
    if stale_before_reload {
        ui.colored_label(
            egui::Color32::from_rgb(190, 104, 18),
            "The source or history changed. Reload these properties before applying.",
        );
        if ui.button("Reload properties").clicked() && actions_allowed {
            *property_draft = property_draft_for(graph, selection, revision);
        }
    }
    let Some(draft) = property_draft.as_mut() else {
        ui.label("This selection is no longer in the preview.");
        return;
    };
    let stale_basis = draft.basis_revision != revision;
    let can_apply = actions_allowed && graph_is_current && !stale_basis;
    let mut operations = Vec::new();
    match &mut draft.fields {
        PropertyFields::Node {
            label,
            shape,
            group,
        } => {
            ui.label(format!("Node {}", selection_label(selection)));
            let label_id =
                ui.make_persistent_id(("diagram-node-label", selection_label(selection)));
            clear_text_edit_undoer(ui.ctx(), label_id);
            ui.add_enabled_ui(actions_allowed && !stale_basis, |ui| {
                ui.label("Label");
                ui.add(egui::TextEdit::singleline(label).id(label_id));
                ui.label("Shape");
                egui::ComboBox::from_id_salt(("diagram-node-shape", selection_label(selection)))
                    .selected_text(shape_label(*shape))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(shape, NodeShape::Rectangle, "Rectangle");
                        ui.selectable_value(shape, NodeShape::Rounded, "Rounded");
                        ui.selectable_value(shape, NodeShape::Diamond, "Diamond");
                    });
                ui.label("Group");
                egui::ComboBox::from_id_salt(("diagram-node-group", selection_label(selection)))
                    .selected_text(group.as_ref().map_or("None", GroupId::as_str))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(group, None, "None");
                        for candidate in &graph.groups {
                            ui.selectable_value(
                                group,
                                Some(candidate.id.clone()),
                                candidate.id.as_str(),
                            );
                        }
                    });
            });
            if let DiagramElement::Node(id) = selection {
                operations.extend([
                    GraphOperation::SetNodeLabel {
                        id: id.clone(),
                        label: label.clone(),
                    },
                    GraphOperation::SetNodeShape {
                        id: id.clone(),
                        shape: *shape,
                    },
                    GraphOperation::SetNodeGroup {
                        id: id.clone(),
                        group: group.clone(),
                    },
                ]);
            }
        }
        PropertyFields::Edge { label, from, to } => {
            ui.label(format!("Edge {}", selection_label(selection)));
            let label_id =
                ui.make_persistent_id(("diagram-edge-label", selection_label(selection)));
            clear_text_edit_undoer(ui.ctx(), label_id);
            let mut text = label.clone().unwrap_or_default();
            ui.add_enabled_ui(actions_allowed && !stale_basis, |ui| {
                ui.label("Label (blank removes label)");
                let response = ui.add(egui::TextEdit::singleline(&mut text).id(label_id));
                if response.changed() {
                    *label = (!text.is_empty()).then(|| text.clone());
                }
                if !graph.nodes.is_empty() {
                    ui.label("From");
                    egui::ComboBox::from_id_salt(("diagram-edge-from", selection_label(selection)))
                        .selected_text(from.as_str())
                        .show_ui(ui, |ui| {
                            for node in &graph.nodes {
                                ui.selectable_value(from, node.id.clone(), node.id.as_str());
                            }
                        });
                    ui.label("To");
                    egui::ComboBox::from_id_salt(("diagram-edge-to", selection_label(selection)))
                        .selected_text(to.as_str())
                        .show_ui(ui, |ui| {
                            for node in &graph.nodes {
                                ui.selectable_value(to, node.id.clone(), node.id.as_str());
                            }
                        });
                }
            });
            if let DiagramElement::Edge(id) = selection {
                operations.extend([
                    GraphOperation::SetEdgeLabel {
                        id: *id,
                        label: label.clone(),
                    },
                    GraphOperation::ReconnectEdge {
                        id: *id,
                        from: from.clone(),
                        to: to.clone(),
                    },
                ]);
            }
        }
        PropertyFields::Group { label } => {
            ui.label(format!("Group {}", selection_label(selection)));
            let label_id =
                ui.make_persistent_id(("diagram-group-label", selection_label(selection)));
            clear_text_edit_undoer(ui.ctx(), label_id);
            ui.add_enabled_ui(actions_allowed && !stale_basis, |ui| {
                ui.label("Label");
                ui.add(egui::TextEdit::singleline(label).id(label_id));
            });
            if let DiagramElement::Group(id) = selection {
                operations.push(GraphOperation::SetGroupLabel {
                    id: id.clone(),
                    label: label.clone(),
                });
            }
        }
    }
    if ui
        .add_enabled(can_apply, egui::Button::new("Apply properties"))
        .clicked()
        && pending.is_none()
        && !operations.is_empty()
    {
        *pending = Some(PendingMutation::Transaction {
            operations,
            expected_revision: draft.basis_revision,
            select_after: Some(draft.target.clone()),
        });
    }
}

fn property_draft_for(
    graph: &DiagramDocument,
    selection: &DiagramElement,
    revision: u64,
) -> Option<PropertyDraft> {
    let fields = match selection {
        DiagramElement::Node(id) => {
            let node = graph.nodes.iter().find(|node| &node.id == id)?;
            PropertyFields::Node {
                label: node.label.clone(),
                shape: node.shape,
                group: node.group.clone(),
            }
        }
        DiagramElement::Edge(id) => {
            let edge = graph.edges.iter().find(|edge| edge.id == *id)?;
            PropertyFields::Edge {
                label: edge.label.clone(),
                from: edge.from.clone(),
                to: edge.to.clone(),
            }
        }
        DiagramElement::Group(id) => {
            let group = graph.groups.iter().find(|group| &group.id == id)?;
            PropertyFields::Group {
                label: group.label.clone(),
            }
        }
    };
    Some(PropertyDraft {
        target: selection.clone(),
        basis_revision: revision,
        fields,
    })
}

fn selection_label(selection: &DiagramElement) -> String {
    match selection {
        DiagramElement::Node(id) => id.as_str().to_owned(),
        DiagramElement::Edge(id) => id.get().to_string(),
        DiagramElement::Group(id) => id.as_str().to_owned(),
    }
}

fn shape_label(shape: NodeShape) -> &'static str {
    match shape {
        NodeShape::Rectangle => "Rectangle",
        NodeShape::Rounded => "Rounded",
        NodeShape::Diamond => "Diamond",
    }
}

fn next_node(
    graph: &DiagramDocument,
    camera_center: ScenePoint,
) -> Result<(NodeId, Position), String> {
    let id = (1..=MAX_DIAGRAM_NODES + 1)
        .find_map(|index| {
            let candidate = NodeId::new(format!("Node{index}")).ok()?;
            (!graph.nodes.iter().any(|node| node.id == candidate)).then_some(candidate)
        })
        .ok_or_else(|| "Could not allocate a unique node ID.".to_string())?;
    let positions = graph
        .nodes
        .iter()
        .map(|node| node.position)
        .collect::<Vec<_>>();
    let center_x = snap_world(camera_center.x);
    let center_y = snap_world(camera_center.y);
    let position = (0_i32..=32)
        .flat_map(|radius| {
            (-radius..=radius).flat_map(move |dy| {
                (-radius..=radius)
                    .filter_map(move |dx| (dx.abs().max(dy.abs()) == radius).then_some((dx, dy)))
            })
        })
        .find_map(|(dx, dy)| {
            let x = center_x + dx as f32 * 180.0;
            let y = center_y + dy as f32 * 180.0;
            let candidate = Position::new(x, y).ok()?;
            positions
                .iter()
                .all(|position| {
                    f64::from(position.x - candidate.x).hypot(f64::from(position.y - candidate.y))
                        >= DRAG_NODE_SEPARATION
                })
                .then_some(candidate)
        })
        .ok_or_else(|| {
            "No separated bounded node position is available near the current view.".to_string()
        })?;
    Ok((id, position))
}

fn next_group_id(graph: &DiagramDocument) -> Result<GroupId, String> {
    (1..=MAX_DIAGRAM_GROUPS + 1)
        .find_map(|index| {
            let candidate = GroupId::new(format!("Group{index}")).ok()?;
            (!graph.groups.iter().any(|group| group.id == candidate)
                && !graph
                    .nodes
                    .iter()
                    .any(|node| node.id.as_str() == candidate.as_str()))
            .then_some(candidate)
        })
        .ok_or_else(|| "Could not allocate a unique group ID.".into())
}

fn removal_operations(
    selection: Option<&DiagramElement>,
    selected_nodes: &BTreeSet<NodeId>,
) -> Vec<GraphOperation> {
    if !selected_nodes.is_empty() {
        return selected_nodes
            .iter()
            .map(|id| GraphOperation::RemoveNode { id: id.clone() })
            .collect();
    }
    match selection {
        Some(DiagramElement::Node(id)) => vec![GraphOperation::RemoveNode { id: id.clone() }],
        Some(DiagramElement::Edge(id)) => vec![GraphOperation::RemoveEdge { id: *id }],
        Some(DiagramElement::Group(id)) => vec![GraphOperation::RemoveGroup { id: id.clone() }],
        None => Vec::new(),
    }
}

fn canvas_element_id(
    interaction: CanvasInteraction,
    kind: &'static str,
    key: impl std::hash::Hash + std::fmt::Debug,
) -> egui::Id {
    match interaction {
        CanvasInteraction::Editor => egui::Id::new((kind, key)),
        CanvasInteraction::Viewer => egui::Id::new(("diagram-studio-viewer", kind, key)),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_canvas(
    ui: &mut egui::Ui,
    graph: Option<&DiagramDocument>,
    scene: Option<&DiagramScene>,
    graph_is_current: bool,
    revision: u64,
    camera: &mut Camera,
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    drag: &mut Option<DragState>,
    actions_allowed: bool,
    interaction: CanvasInteraction,
    pending: &mut Option<PendingMutation>,
    message: &mut Option<StudioNotice>,
    canvas_focus_requested: &mut Option<egui::Id>,
    #[cfg(test)] test_canvas_id: &mut Option<egui::Id>,
) -> egui::Rect {
    let edit_actions_allowed = actions_allowed && interaction == CanvasInteraction::Editor;
    let width = ui.available_width().max(CANVAS_MIN_WIDTH);
    let height = ui.available_height().max(CANVAS_MIN_HEIGHT);
    let (canvas_response, painter) =
        ui.allocate_painter(egui::vec2(width, height), egui::Sense::click_and_drag());
    let canvas_rect = canvas_response.rect;
    let canvas_id = canvas_response.id;
    #[cfg(test)]
    {
        *test_canvas_id = Some(canvas_id);
    }
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(canvas_rect));
    painter.rect_filled(canvas_rect, 0.0, egui::Color32::from_rgb(248, 249, 251));

    let pointer = ui.ctx().input(|input| input.pointer.interact_pos());
    if actions_allowed
        && let Some(pointer) = pointer.filter(|pointer| canvas_rect.contains(*pointer))
    {
        let scroll = ui.ctx().input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > 0.01 {
            camera.zoom =
                (camera.zoom * (scroll * 0.001).exp()).clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
        }
        let _ = pointer;
    }
    if actions_allowed && canvas_response.dragged_by(egui::PointerButton::Middle) {
        let delta = canvas_response.drag_delta();
        camera.center.x -= f64::from(delta.x / camera.zoom);
        camera.center.y -= f64::from(delta.y / camera.zoom);
    }

    let mut entity_selected = false;
    if let (Some(scene), Some(bounds)) = (scene, scene.and_then(|scene| scene.bounds)) {
        if camera.fit_next {
            fit_camera(camera, bounds, canvas_rect);
        }
        let drag_delta = drag.as_ref().and_then(|drag| {
            pointer.map(|pointer| ScenePoint {
                x: f64::from(pointer.x - drag.pointer_origin.x) / f64::from(camera.zoom),
                y: f64::from(pointer.y - drag.pointer_origin.y) / f64::from(camera.zoom),
            })
        });
        for group in &scene.groups {
            let group_rect = world_rect_to_screen(group.bounds, canvas_rect, *camera);
            let selected = selection == &Some(DiagramElement::Group(group.id.clone()));
            painter.rect_stroke(
                group_rect,
                10.0,
                egui::Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected {
                        egui::Color32::from_rgb(55, 109, 189)
                    } else {
                        egui::Color32::from_rgb(150, 160, 176)
                    },
                ),
                egui::StrokeKind::Inside,
            );
            let label_rect = world_rect_to_screen(group.label_bounds, canvas_rect, *camera);
            draw_prepared_text(
                &painter,
                label_rect,
                &group.label,
                group.label_alignment,
                egui::Color32::from_rgb(85, 94, 106),
                scene.metrics,
                camera.zoom,
            );
            let header_hit = label_rect.expand(4.0).intersect(canvas_rect);
            let response = ui.interact(
                header_hit,
                canvas_element_id(interaction, "diagram-group", group.id.as_str()),
                egui::Sense::click(),
            );
            if edit_actions_allowed && response.clicked() {
                entity_selected = true;
                select_canvas_element(
                    selection,
                    selected_nodes,
                    DiagramElement::Group(group.id.clone()),
                    false,
                    canvas_id,
                    canvas_focus_requested,
                );
            }
        }

        for edge in &scene.edges {
            let points = cubic_points(edge.path, canvas_rect, *camera, 20);
            if points.len() >= 2 {
                let selected = selection == &Some(DiagramElement::Edge(edge.id));
                let color = if selected {
                    egui::Color32::from_rgb(44, 105, 190)
                } else {
                    egui::Color32::from_rgb(85, 92, 103)
                };
                painter.add(egui::Shape::line(
                    points.clone(),
                    egui::Stroke::new(if selected { 2.5 } else { 1.6 }, color),
                ));
                let tip = world_to_screen(edge.arrowhead.tip, canvas_rect, *camera);
                let left = world_to_screen(edge.arrowhead.left, canvas_rect, *camera);
                let right = world_to_screen(edge.arrowhead.right, canvas_rect, *camera);
                painter.line_segment([tip, left], egui::Stroke::new(1.6, color));
                painter.line_segment([tip, right], egui::Stroke::new(1.6, color));
                if let (Some(label), Some(label_bounds), Some(alignment)) =
                    (edge.label.as_ref(), edge.label_bounds, edge.label_alignment)
                {
                    draw_prepared_text(
                        &painter,
                        world_rect_to_screen(label_bounds, canvas_rect, *camera),
                        label,
                        alignment,
                        color,
                        scene.metrics,
                        camera.zoom,
                    );
                }
                if let Some(hit_rect) = points_bounds(&points) {
                    let response = ui.interact(
                        hit_rect.expand(8.0).intersect(canvas_rect),
                        canvas_element_id(interaction, "diagram-edge", edge.id.get()),
                        egui::Sense::click(),
                    );
                    if edit_actions_allowed && response.clicked() {
                        entity_selected = true;
                        select_canvas_element(
                            selection,
                            selected_nodes,
                            DiagramElement::Edge(edge.id),
                            false,
                            canvas_id,
                            canvas_focus_requested,
                        );
                    }
                }
            }
        }

        if let Some(graph) = graph {
            for node in &scene.nodes {
                let is_being_dragged = drag
                    .as_ref()
                    .is_some_and(|drag| drag.initial_positions.iter().any(|p| p.id == node.id));
                let translated = if is_being_dragged {
                    drag_delta.map_or(node.bounds, |delta| translate_rect(node.bounds, delta))
                } else {
                    node.bounds
                };
                let node_rect = world_rect_to_screen(translated, canvas_rect, *camera);
                let selected = selected_nodes.contains(&node.id);
                let fill = if selected {
                    egui::Color32::from_rgb(222, 235, 252)
                } else {
                    egui::Color32::WHITE
                };
                let stroke = if selected {
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(50, 108, 190))
                } else {
                    egui::Stroke::new(1.2, egui::Color32::from_rgb(81, 91, 106))
                };
                match node.shape {
                    NodeShape::Rectangle => {
                        painter.rect_filled(node_rect, 2.0, fill);
                        painter.rect_stroke(node_rect, 2.0, stroke, egui::StrokeKind::Inside);
                    }
                    NodeShape::Rounded => {
                        painter.rect_filled(node_rect, 12.0, fill);
                        painter.rect_stroke(node_rect, 12.0, stroke, egui::StrokeKind::Inside);
                    }
                    NodeShape::Diamond => {
                        let center = node_rect.center();
                        let vertices = vec![
                            egui::pos2(center.x, node_rect.top()),
                            egui::pos2(node_rect.right(), center.y),
                            egui::pos2(center.x, node_rect.bottom()),
                            egui::pos2(node_rect.left(), center.y),
                        ];
                        painter.add(egui::Shape::convex_polygon(vertices, fill, stroke));
                    }
                }
                let label_bounds = if is_being_dragged {
                    drag_delta.map_or(node.label_bounds, |delta| {
                        translate_rect(node.label_bounds, delta)
                    })
                } else {
                    node.label_bounds
                };
                let label_rect = world_rect_to_screen(label_bounds, canvas_rect, *camera);
                draw_prepared_text(
                    &painter,
                    label_rect,
                    &node.label,
                    node.label_alignment,
                    egui::Color32::from_rgb(34, 39, 47),
                    scene.metrics,
                    camera.zoom,
                );
                let response = ui.interact(
                    node_rect,
                    canvas_element_id(interaction, "diagram-node", node.id.as_str()),
                    egui::Sense::click_and_drag(),
                );
                if edit_actions_allowed && response.clicked() {
                    entity_selected = true;
                    let extend = ui
                        .ctx()
                        .input(|input| input.modifiers.shift || input.modifiers.command);
                    select_canvas_element(
                        selection,
                        selected_nodes,
                        DiagramElement::Node(node.id.clone()),
                        extend,
                        canvas_id,
                        canvas_focus_requested,
                    );
                }
                if edit_actions_allowed && response.drag_started() {
                    entity_selected = true;
                    let extend = ui
                        .ctx()
                        .input(|input| input.modifiers.shift || input.modifiers.command);
                    if !selected_nodes.contains(&node.id) {
                        select_canvas_element(
                            selection,
                            selected_nodes,
                            DiagramElement::Node(node.id.clone()),
                            extend,
                            canvas_id,
                            canvas_focus_requested,
                        );
                    } else {
                        *canvas_focus_requested = Some(canvas_id);
                    }
                    let initial_positions = graph
                        .nodes
                        .iter()
                        .filter(|candidate| selected_nodes.contains(&candidate.id))
                        .map(|candidate| NodePosition {
                            id: candidate.id.clone(),
                            position: candidate.position,
                        })
                        .collect::<Vec<_>>();
                    if let Some(pointer) = response.interact_pointer_pos() {
                        *drag = Some(DragState {
                            active_node: node.id.clone(),
                            pointer_origin: pointer,
                            initial_positions,
                        });
                    }
                }
                let drag_stopped = drag
                    .as_ref()
                    .is_some_and(|drag| drag.active_node == node.id)
                    && response.drag_stopped();
                if drag_stopped {
                    if edit_actions_allowed
                        && graph_is_current
                        && pending.is_none()
                        && let Some(drag_state) = drag.as_ref()
                    {
                        let pointer = response
                            .interact_pointer_pos()
                            .or_else(|| ui.ctx().input(|input| input.pointer.interact_pos()));
                        if let Some(pointer) = pointer {
                            let delta = ScenePoint {
                                x: f64::from(pointer.x - drag_state.pointer_origin.x)
                                    / f64::from(camera.zoom),
                                y: f64::from(pointer.y - drag_state.pointer_origin.y)
                                    / f64::from(camera.zoom),
                            };
                            match moved_positions(&drag_state.initial_positions, delta) {
                                Ok(positions) => {
                                    *pending = Some(PendingMutation::Transaction {
                                        operations: vec![GraphOperation::MoveNodes { positions }],
                                        expected_revision: revision,
                                        select_after: None,
                                    });
                                }
                                Err(error) => *message = Some(StudioNotice::error(error)),
                            }
                        }
                    }
                    *drag = None;
                }
            }
        }
    } else {
        camera.fit_next = false;
        let message = if graph.is_none() {
            "No preview is available. Fix the Mermaid source to create a graph."
        } else {
            "The graph has no drawable nodes yet. Add a node to begin."
        };
        painter.text(
            canvas_rect.center(),
            egui::Align2::CENTER_CENTER,
            message,
            egui::FontId::proportional(14.0),
            egui::Color32::from_rgb(88, 95, 105),
        );
    }

    if edit_actions_allowed && canvas_response.clicked() {
        *canvas_focus_requested = Some(canvas_id);
        if !canvas_response.drag_started() && !entity_selected {
            *selection = None;
            selected_nodes.clear();
        }
    }
    if edit_actions_allowed {
        canvas_shortcuts(
            ui,
            canvas_id,
            camera,
            graph,
            revision,
            selection,
            selected_nodes,
            pending,
            message,
        );
    } else if actions_allowed && interaction == CanvasInteraction::Viewer {
        let pointer_clicked_in_canvas = ui.ctx().input(|input| {
            input.pointer.any_click()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pointer| canvas_rect.contains(pointer))
        });
        if canvas_response.clicked() || pointer_clicked_in_canvas {
            *canvas_focus_requested = Some(canvas_id);
        }
        viewer_canvas_shortcuts(ui, canvas_id, camera);
    }
    let _ = graph_is_current;
    canvas_rect
}

fn fit_camera(camera: &mut Camera, bounds: SceneRect, canvas_rect: egui::Rect) {
    let usable = canvas_rect.size() - egui::vec2(48.0, 48.0);
    let width = bounds.width().max(1.0) as f32;
    let height = bounds.height().max(1.0) as f32;
    camera.center = bounds.center();
    camera.zoom = (usable.x / width)
        .min(usable.y / height)
        .clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
    camera.fit_next = false;
}

fn world_to_screen(point: ScenePoint, canvas: egui::Rect, camera: Camera) -> egui::Pos2 {
    egui::pos2(
        canvas.center().x + ((point.x - camera.center.x) as f32 * camera.zoom),
        canvas.center().y + ((point.y - camera.center.y) as f32 * camera.zoom),
    )
}

fn world_rect_to_screen(rect: SceneRect, canvas: egui::Rect, camera: Camera) -> egui::Rect {
    egui::Rect::from_min_max(
        world_to_screen(rect.min, canvas, camera),
        world_to_screen(rect.max, canvas, camera),
    )
}

fn translate_rect(rect: SceneRect, delta: ScenePoint) -> SceneRect {
    SceneRect {
        min: ScenePoint {
            x: rect.min.x + delta.x,
            y: rect.min.y + delta.y,
        },
        max: ScenePoint {
            x: rect.max.x + delta.x,
            y: rect.max.y + delta.y,
        },
    }
}

fn cubic_points(
    curve: crate::diagram::CubicBezier,
    canvas: egui::Rect,
    camera: Camera,
    segments: usize,
) -> Vec<egui::Pos2> {
    let mut points = Vec::with_capacity(segments.saturating_add(1));
    for index in 0..=segments {
        let t = index as f64 / segments.max(1) as f64;
        let inverse = 1.0 - t;
        let point = ScenePoint {
            x: curve.start.x * inverse.powi(3)
                + 3.0 * curve.control1.x * inverse.powi(2) * t
                + 3.0 * curve.control2.x * inverse * t.powi(2)
                + curve.end.x * t.powi(3),
            y: curve.start.y * inverse.powi(3)
                + 3.0 * curve.control1.y * inverse.powi(2) * t
                + 3.0 * curve.control2.y * inverse * t.powi(2)
                + curve.end.y * t.powi(3),
        };
        points.push(world_to_screen(point, canvas, camera));
    }
    points
}

fn points_bounds(points: &[egui::Pos2]) -> Option<egui::Rect> {
    let first = *points.first()?;
    let mut min = first;
    let mut max = first;
    for point in points.iter().skip(1) {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    Some(egui::Rect::from_min_max(min, max))
}

fn draw_prepared_text(
    painter: &egui::Painter,
    bounds: egui::Rect,
    text: &PreparedText,
    alignment: TextHorizontalAlignment,
    color: egui::Color32,
    metrics: SceneMetrics,
    zoom: f32,
) {
    let inherited_clip = painter.clip_rect();
    for (index, line) in text.lines.iter().enumerate() {
        let width = (line.reserved_width as f32 * zoom).max(0.0);
        let height = (text.line_height as f32 * zoom).max(0.0);
        let x = match alignment {
            TextHorizontalAlignment::Center => bounds.center().x - width / 2.0,
            TextHorizontalAlignment::Leading => bounds.left(),
        };
        let y = bounds.top() + index as f32 * height;
        let line_clip = prepared_text_line_clip(
            egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, height)),
            bounds,
            inherited_clip,
        );
        if !line_clip.is_positive() {
            continue;
        }
        let font_size = ((metrics.text.max_scalar_advance as f32 * zoom) * 0.78).max(1.0);
        let galley = painter.layout_no_wrap(
            line.text.clone(),
            egui::FontId::proportional(font_size),
            color,
        );
        painter
            .with_clip_rect(line_clip)
            .galley(egui::pos2(x, y), galley, color);
    }
}

fn prepared_text_line_clip(
    line: egui::Rect,
    bounds: egui::Rect,
    inherited_clip: egui::Rect,
) -> egui::Rect {
    line.intersect(bounds).intersect(inherited_clip)
}

fn moved_positions(
    original: &[NodePosition],
    delta: ScenePoint,
) -> Result<Vec<NodePosition>, String> {
    let delta_x = snap_delta(delta.x);
    let delta_y = snap_delta(delta.y);
    original
        .iter()
        .map(|target| {
            let position = Position::new(target.position.x + delta_x, target.position.y + delta_y)
                .map_err(|error| {
                    format!("Move would leave the supported coordinate range: {error}")
                })?;
            Ok(NodePosition {
                id: target.id.clone(),
                position,
            })
        })
        .collect()
}

fn snap_delta(value: f64) -> f32 {
    ((value / f64::from(GRID_SPACING)).round() * f64::from(GRID_SPACING)) as f32
}

fn snap_world(value: f64) -> f32 {
    let snapped = (value / f64::from(GRID_SPACING)).round() * f64::from(GRID_SPACING);
    snapped.clamp(
        -f64::from(crate::diagram::MAX_POSITION_ABS),
        f64::from(crate::diagram::MAX_POSITION_ABS),
    ) as f32
}

fn take_history_shortcut(ui: &mut egui::Ui) -> Option<PendingMutation> {
    ui.input_mut(|input| {
        if input.consume_key(
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::Z,
        ) {
            Some(PendingMutation::Redo)
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y) {
            Some(PendingMutation::Redo)
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z) {
            Some(PendingMutation::Undo)
        } else {
            None
        }
    })
}

fn menu_focus_request_for_ui(
    ui: &mut egui::Ui,
    scope: Option<&MenuFocusScope>,
    previous: Option<&MenuFocusScope>,
    actions_allowed: bool,
    closing_requested: bool,
    focused_at_start: Option<egui::Id>,
) -> Option<MenuFocusRequest> {
    if let Some(scope) = scope {
        if !actions_allowed || closing_requested || scope.sizing_pass || scope.item_ids.is_empty() {
            return None;
        }
        let navigation = ui.input_mut(|input| {
            if consume_menu_key(input, egui::Key::Tab, egui::Modifiers::SHIFT) {
                Some(false)
            } else if consume_menu_key(input, egui::Key::Tab, egui::Modifiers::NONE)
                || consume_menu_key(input, egui::Key::ArrowDown, egui::Modifiers::NONE)
                || consume_menu_key(input, egui::Key::ArrowRight, egui::Modifiers::NONE)
            {
                Some(true)
            } else if consume_menu_key(input, egui::Key::ArrowUp, egui::Modifiers::NONE)
                || consume_menu_key(input, egui::Key::ArrowLeft, egui::Modifiers::NONE)
            {
                Some(false)
            } else {
                None
            }
        });
        // Clear egui's pass-level traversal so it cannot move focus behind the popup.
        if navigation.is_some() {
            ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
        }
        let focused_index = scope
            .item_ids
            .iter()
            .position(|id| Some(*id) == focused_at_start);
        let index = match (focused_index, navigation) {
            (Some(index), Some(true)) => Some((index + 1) % scope.item_ids.len()),
            (Some(index), Some(false)) => Some(if index == 0 {
                scope.item_ids.len() - 1
            } else {
                index - 1
            }),
            (None, Some(true)) | (None, None) => Some(0),
            (None, Some(false)) => Some(scope.item_ids.len() - 1),
            (Some(_), None) => None,
        }?;
        Some(MenuFocusRequest {
            popup_id: scope.popup_id,
            destination: MenuFocusDestination::Item(scope.item_ids[index]),
        })
    } else if actions_allowed && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        previous.map(|scope| MenuFocusRequest {
            popup_id: scope.popup_id,
            destination: MenuFocusDestination::Trigger(scope.trigger_id),
        })
    } else {
        None
    }
}

fn consume_menu_key(
    input: &mut egui::InputState,
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> bool {
    let mut consumed = false;
    input.events.retain(|event| {
        let matches = matches!(
            event,
            egui::Event::Key {
                key: event_key,
                pressed: true,
                modifiers: event_modifiers,
                ..
            } if *event_key == key && *event_modifiers == modifiers
        );
        consumed |= matches;
        !matches
    });
    consumed
}

fn canvas_shortcuts(
    ui: &mut egui::Ui,
    canvas_id: egui::Id,
    camera: &mut Camera,
    graph: Option<&DiagramDocument>,
    revision: u64,
    selection: &mut Option<DiagramElement>,
    selected_nodes: &mut BTreeSet<NodeId>,
    pending: &mut Option<PendingMutation>,
    message: &mut Option<StudioNotice>,
) {
    let canvas_focused = ui.memory(|memory| memory.has_focus(canvas_id));
    if !canvas_focused {
        return;
    }
    enum Action {
        SelectAll,
        Remove,
        Move(i32, i32),
        Pan(f64, f64),
        ZoomIn,
        ZoomOut,
    }
    let action = ui.input_mut(|input| {
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::A) {
            return Some(Action::SelectAll);
        }
        if input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
            || input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
        {
            return Some(Action::Remove);
        }
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowLeft) {
            return Some(Action::Pan(-1.0, 0.0));
        }
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowRight) {
            return Some(Action::Pan(1.0, 0.0));
        }
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowUp) {
            return Some(Action::Pan(0.0, -1.0));
        }
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowDown) {
            return Some(Action::Pan(0.0, 1.0));
        }
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Plus)
            || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Equals)
        {
            return Some(Action::ZoomIn);
        }
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Minus) {
            return Some(Action::ZoomOut);
        }
        if input.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowLeft) {
            return Some(Action::Move(-40, 0));
        }
        if input.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowRight) {
            return Some(Action::Move(40, 0));
        }
        if input.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowUp) {
            return Some(Action::Move(0, -40));
        }
        if input.consume_key(egui::Modifiers::SHIFT, egui::Key::ArrowDown) {
            return Some(Action::Move(0, 40));
        }
        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
            return Some(Action::Move(-10, 0));
        }
        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
            return Some(Action::Move(10, 0));
        }
        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            return Some(Action::Move(0, -10));
        }
        if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
            return Some(Action::Move(0, 10));
        }
        None
    });
    match action {
        Some(Action::SelectAll) => {
            if let Some(graph) = graph {
                selected_nodes.clear();
                selected_nodes.extend(graph.nodes.iter().map(|node| node.id.clone()));
                *selection = selected_nodes.last().cloned().map(DiagramElement::Node);
            }
        }
        Some(Action::Remove) if pending.is_none() => {
            if graph.is_some() {
                let operations = removal_operations(selection.as_ref(), selected_nodes);
                if !operations.is_empty() {
                    *pending = Some(PendingMutation::Transaction {
                        operations,
                        expected_revision: revision,
                        select_after: None,
                    });
                }
            }
        }
        Some(Action::Move(dx, dy)) if pending.is_none() && !selected_nodes.is_empty() => {
            if let Some(graph) = graph {
                let initial = graph
                    .nodes
                    .iter()
                    .filter(|node| selected_nodes.contains(&node.id))
                    .map(|node| NodePosition {
                        id: node.id.clone(),
                        position: node.position,
                    })
                    .collect::<Vec<_>>();
                if !initial.is_empty() {
                    match moved_positions(
                        &initial,
                        ScenePoint {
                            x: f64::from(dx),
                            y: f64::from(dy),
                        },
                    ) {
                        Ok(positions) => {
                            *pending = Some(PendingMutation::Transaction {
                                operations: vec![GraphOperation::MoveNodes { positions }],
                                expected_revision: revision,
                                select_after: None,
                            });
                        }
                        Err(error) => *message = Some(StudioNotice::error(error)),
                    }
                }
            }
        }
        Some(Action::Pan(dx, dy)) if pending.is_none() => {
            let world_step = CAMERA_KEYBOARD_PAN_SCREEN_POINTS / f64::from(camera.zoom);
            let limit = f64::from(crate::diagram::MAX_POSITION_ABS);
            camera.center.x = (camera.center.x + dx * world_step).clamp(-limit, limit);
            camera.center.y = (camera.center.y + dy * world_step).clamp(-limit, limit);
        }
        Some(Action::ZoomIn) if pending.is_none() => {
            camera.zoom =
                (camera.zoom * CAMERA_KEYBOARD_ZOOM_FACTOR).clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
        }
        Some(Action::ZoomOut) if pending.is_none() => {
            camera.zoom =
                (camera.zoom / CAMERA_KEYBOARD_ZOOM_FACTOR).clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
        }
        _ => {}
    }
}

fn draw_drawio_workflow_panel(
    ui: &mut egui::Ui,
    work: Option<&PendingDrawioWork>,
    review: Option<&DrawioWorkflowReview>,
    review_scope_current: bool,
    actions_allowed: bool,
    document_intent: &mut Option<DocumentIntent>,
    #[cfg(test)] test_action_rects: &mut Vec<(String, egui::Rect, egui::Rect)>,
) {
    #[cfg(test)]
    let mut record = |name: &str, response: &egui::Response, clip: egui::Rect| {
        test_action_rects.push((name.to_owned(), response.rect, clip));
    };
    #[cfg(not(test))]
    let record = |_name: &str, _response: &egui::Response, _clip: egui::Rect| {};

    if let Some(work) = work {
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                let activity = match &work.kind {
                    DrawioWorkKind::Import { source_path } => format!(
                        "Reading and reviewing {}…",
                        source_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("draw.io file")
                    ),
                    DrawioWorkKind::Export => "Preparing the committed visual scene…".into(),
                };
                ui.label(if work.retired {
                    "Waiting for a retired draw.io operation to stop; its result will be discarded."
                } else {
                    &activity
                });
                let cancel = ui.add_enabled(
                    actions_allowed && !work.retired,
                    egui::Button::new("Cancel Draw.io"),
                );
                record("drawio_cancel", &cancel, ui.clip_rect());
                if cancel.clicked() && document_intent.is_none() {
                    *document_intent = Some(DocumentIntent::DrawioCancel);
                }
            });
        });
        return;
    }

    let Some(review) = review else {
        return;
    };
    let scope_live = review_scope_current;
    ui.group(|ui| {
        ui.horizontal_wrapped(|ui| {
            match review {
                DrawioWorkflowReview::Import {
                    source_path,
                    candidate,
                    copy_path,
                    ..
                } => {
                    let scene = candidate.document.scene();
                    let imported_object_count = scene
                        .objects
                        .iter()
                        .filter(|object| object.id != crate::diagram::TITLE_OBJECT_ID)
                        .count();
                    ui.strong("Review draw.io import");
                    ui.label(format!(
                        "{} objects in {} layers from {}; source remains unchanged.",
                        imported_object_count,
                        scene.layers.len(),
                        source_path.file_name().and_then(|name| name.to_str()).unwrap_or("draw.io file")
                    ));
                    if !scope_live {
                        ui.colored_label(
                            egui::Color32::from_rgb(185, 65, 65),
                            "The active document or owner changed; this candidate cannot be published.",
                        );
                    }
                    if let Some(path) = copy_path {
                        ui.label(format!("Copy created at {}.", path.display()));
                        let open = ui.add_enabled(
                            actions_allowed && scope_live && document_intent.is_none(),
                            egui::Button::new("Open copy…"),
                        );
                        record("drawio_open_copy", &open, ui.clip_rect());
                        if open.clicked() {
                            *document_intent = Some(DocumentIntent::DrawioOpenImportCopy);
                        }
                    } else {
                        let publish = ui.add_enabled(
                            actions_allowed && scope_live && document_intent.is_none(),
                            egui::Button::new("Create .mld copy…"),
                        );
                        record("drawio_publish_import", &publish, ui.clip_rect());
                        if publish.clicked() {
                            *document_intent = Some(DocumentIntent::DrawioPublishImport);
                        }
                    }
                }
                DrawioWorkflowReview::ExportReady { result, .. } => {
                    ui.strong("Review draw.io export");
                    ui.label(format!(
                        "Complete export of the committed visual scene ({} bytes).",
                        result.bytes.len(),
                    ));
                    if !scope_live {
                        ui.colored_label(
                            egui::Color32::from_rgb(185, 65, 65),
                            "The visual document or owner changed; this export cannot be published.",
                        );
                    }
                    let publish = ui.add_enabled(
                        actions_allowed && scope_live && document_intent.is_none(),
                        egui::Button::new("Save .drawio…"),
                    );
                    record("drawio_publish_export", &publish, ui.clip_rect());
                    if publish.clicked() {
                        *document_intent = Some(DocumentIntent::DrawioPublishExport);
                    }
                }
                DrawioWorkflowReview::ImportUnsupported { source_path, .. } => {
                    ui.strong("Draw.io import is not available for this file");
                    ui.label(format!(
                        "{} contains unsupported content; no candidate was created.",
                        source_path.display()
                    ));
                }
                DrawioWorkflowReview::ExportUnsupported {
                    reason,
                    ..
                } => {
                    ui.strong("Draw.io export is not available for this visual scene");
                    ui.label(reason);
                }
            }
            let cancel = ui.add_enabled(
                actions_allowed && document_intent.is_none(),
                egui::Button::new("Discard review"),
            );
            record("drawio_cancel", &cancel, ui.clip_rect());
            if cancel.clicked() {
                *document_intent = Some(DocumentIntent::DrawioCancel);
            }
        });
        match review {
            DrawioWorkflowReview::Import { candidate, .. } => {
                draw_drawio_report(ui, &candidate.report)
            }
            DrawioWorkflowReview::ExportReady { result, .. } => {
                draw_drawio_report(ui, &result.report)
            }
            DrawioWorkflowReview::ImportUnsupported { report, .. } => {
                draw_drawio_report(ui, report)
            }
            DrawioWorkflowReview::ExportUnsupported { report, .. } => {
                draw_drawio_report(ui, report)
            }
        }
    });
}

fn draw_drawio_report(ui: &mut egui::Ui, report: &DrawioReport) {
    if report.total_issues == 0 {
        ui.label("No unsupported or inferred content was reported.");
        return;
    }
    ui.colored_label(
        egui::Color32::from_rgb(185, 110, 35),
        format!(
            "{} item or property issue(s) require review:",
            report.total_issues
        ),
    );
    let shown = report.issues.len().min(6);
    for issue in report.issues.iter().take(shown) {
        match issue.source_id.as_deref() {
            Some(source_id) => ui.label(format!("{source_id}: {}", issue.property)),
            None => ui.label(issue.property),
        };
    }
    let omitted = report.total_issues.saturating_sub(shown);
    if omitted > 0 {
        ui.weak(format!(
            "{omitted} additional issue(s) are omitted from this summary."
        ));
    }
}

fn draw_recent_documents(
    ui: &mut egui::Ui,
    index: Option<&RecentDiagramIndex>,
    loaded: bool,
    load_error: Option<&str>,
    status: Option<&str>,
    search: &mut String,
    expanded: bool,
    actions_allowed: bool,
    intent: &mut Option<DocumentIntent>,
    #[cfg(test)] action_rects: &mut Vec<(String, egui::Rect, egui::Rect)>,
) -> Option<RecentPanelEffects> {
    let toggle = ui.add_enabled(
        actions_allowed,
        egui::Button::new(if expanded {
            "Recent diagrams ▾"
        } else {
            "Recent diagrams ▸"
        }),
    );
    #[cfg(test)]
    action_rects.push(("recent_panel".into(), toggle.rect, ui.clip_rect()));
    let mut effects = RecentPanelEffects {
        open: toggle.clicked().then_some(!expanded),
        search: None,
    };
    if !expanded {
        return effects.open.is_some().then_some(effects);
    }
    if !loaded {
        ui.weak("Loading recent diagrams…");
        return effects.open.is_some().then_some(effects);
    }
    let mut query = search.clone();
    ui.horizontal(|ui| {
        ui.label("Find");
        ui.add_enabled(actions_allowed, egui::TextEdit::singleline(&mut query));
    });
    if actions_allowed && query != *search {
        effects.search = Some(query.clone());
    }
    if let Some(error) = load_error {
        ui.colored_label(
            egui::Color32::from_rgb(185, 65, 65),
            format!("Recent list was preserved but could not be read: {error}"),
        );
        return (effects.open.is_some() || effects.search.is_some()).then_some(effects);
    }
    if let Some(error) = status {
        ui.weak(format!(
            "The latest document action succeeded, but recents were not saved: {error}"
        ));
    }
    let rows = index
        .as_ref()
        .map(|index| index.matching(&query).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    if rows.is_empty() {
        ui.weak("No recent diagrams match this search.");
        return (effects.open.is_some() || effects.search.is_some()).then_some(effects);
    }
    egui::ScrollArea::vertical()
        .max_height(170.0)
        .id_salt("diagram-studio-recents")
        .show(ui, |ui| {
            for path in rows.iter() {
                #[cfg(test)]
                let row = rows
                    .iter()
                    .position(|candidate| candidate == path)
                    .expect("rendered recent path must be in the displayed rows");
                ui.push_id(path, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(path.to_string_lossy());
                        let open = ui.add_enabled(actions_allowed, egui::Button::new("Open"));
                        #[cfg(test)]
                        action_rects.push((
                            format!("recent_open_{row}"),
                            open.rect,
                            ui.clip_rect(),
                        ));
                        if open.clicked() && intent.is_none() {
                            *intent = Some(DocumentIntent::OpenRecent(path.clone()));
                        }
                        let duplicate =
                            ui.add_enabled(actions_allowed, egui::Button::new("Duplicate…"));
                        #[cfg(test)]
                        action_rects.push((
                            format!("recent_duplicate_{row}"),
                            duplicate.rect,
                            ui.clip_rect(),
                        ));
                        if duplicate.clicked() && intent.is_none() {
                            *intent = Some(DocumentIntent::DuplicateRecent(path.clone()));
                        }
                    });
                });
            }
        });
    (effects.open.is_some() || effects.search.is_some()).then_some(effects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    const TEST_VIEWPORT_SIZE: egui::Vec2 = egui::vec2(1200.0, 900.0);

    fn write_test_png(path: &Path, width: u32, height: u32) {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([42, 107, 188, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        std::fs::write(path, bytes.into_inner()).unwrap();
    }

    const DRAWIO_WORKFLOW_FIXTURE: &str = "<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"layer\" parent=\"0\" value=\"Layer\"/><mxCell id=\"group\" style=\"group;\" vertex=\"1\" parent=\"layer\"><mxGeometry x=\"30\" y=\"40\" width=\"300\" height=\"200\" as=\"geometry\"/></mxCell><mxCell id=\"box\" value=\"Imported box\" style=\"shape=rectangle;\" vertex=\"1\" parent=\"group\"><mxGeometry x=\"10\" y=\"20\" width=\"100\" height=\"50\" as=\"geometry\"/></mxCell></root></mxGraphModel>";

    fn wait_for_drawio_review(dialog: &mut DiagramStudioDialog, ctx: &egui::Context) {
        for _ in 0..200 {
            frame(dialog, ctx);
            if dialog.drawio_work.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            dialog.drawio_work.is_none(),
            "draw.io worker settles within the bounded test wait"
        );
        assert!(
            dialog.drawio_review.is_some(),
            "worker should produce a review: {:?}",
            dialog.message.as_ref().map(StudioNotice::text)
        );
    }

    fn click_mermaid_drawio_import(dialog: &mut DiagramStudioDialog, ctx: &egui::Context) {
        click_action(dialog, ctx, "menu_import");
        click_action(dialog, ctx, "drawio_import");
    }

    fn click_visual_drawio_export(dialog: &mut DiagramStudioDialog, ctx: &egui::Context) {
        click_action(dialog, ctx, "menu_drawio");
        click_action(dialog, ctx, "drawio_export");
    }

    fn frame(dialog: &mut DiagramStudioDialog, ctx: &egui::Context) {
        frame_with_root_input(dialog, ctx, true);
    }

    fn frame_with_root_input(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        root_input_allowed: bool,
    ) {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    TEST_VIEWPORT_SIZE,
                )),
                focused: true,
                ..Default::default()
            };
            let output = ctx.run_ui(raw_input, |ui| {
                let first_pass = ui.ctx().current_pass_index() == 0;
                dialog.show(ui.ctx(), first_pass, true, root_input_allowed);
            });
            output.drop_without_applying_deltas();
            dialog.on_ui_frame_complete(ctx);
        });
    }

    fn frame_with_root_events(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        TEST_VIEWPORT_SIZE,
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| dialog.show(ui.ctx(), ui.current_pass_index() == 0, true, true),
            );
            output.drop_without_applying_deltas();
            dialog.on_ui_frame_complete(ctx);
        });
    }

    fn click_root_action(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        name: &str,
        layer: egui::LayerId,
    ) {
        frame(dialog, ctx); // Mount the window, which may require an invisible sizing pass.
        frame(dialog, ctx);
        assert_eq!(
            dialog.test_action_enabled(name),
            Some(true),
            "{name} must be visible and enabled"
        );
        let click = action_center(dialog, name);
        assert_eq!(
            ctx.layer_id_at(click),
            Some(layer),
            "{name} must hit its host"
        );
        click_root_position(dialog, ctx, click);
    }

    fn click_root_position(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        click: egui::Pos2,
    ) {
        for pressed in [true, false] {
            frame_with_root_events(
                dialog,
                ctx,
                vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    fn frame_with_size(dialog: &mut DiagramStudioDialog, ctx: &egui::Context, size: egui::Vec2) {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                focused: true,
                ..Default::default()
            };
            let output = ctx.run_ui(raw_input, |ui| {
                let first_pass = ui.ctx().current_pass_index() == 0;
                dialog.show(ui.ctx(), first_pass, true, true);
            });
            output.drop_without_applying_deltas();
            dialog.on_ui_frame_complete(ctx);
        });
    }

    fn frame_with_viewer_size(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        size: egui::Vec2,
    ) {
        super::super::render::with_immediate_viewport_test_size(viewer_viewport_id(), size, || {
            frame(dialog, ctx)
        });
    }

    fn frame_with_viewport_commands(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
    ) -> Vec<egui::ViewportCommand> {
        frame_with_viewport_commands_for(dialog, ctx, viewport_id())
    }

    fn frame_with_viewport_commands_for(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        target_viewport: egui::ViewportId,
    ) -> Vec<egui::ViewportCommand> {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    TEST_VIEWPORT_SIZE,
                )),
                focused: true,
                ..Default::default()
            };
            let output = ctx.run_ui(raw_input, |ui| {
                let first_pass = ui.ctx().current_pass_index() == 0;
                dialog.show(ui.ctx(), first_pass, true, true);
            });
            let commands = output
                .viewport_output
                .get(&target_viewport)
                .map(|output| output.commands.clone())
                .unwrap_or_default();
            output.drop_without_applying_deltas();
            dialog.on_ui_frame_complete(ctx);
            commands
        })
    }

    fn frame_with_child_events(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        super::super::render::with_immediate_viewport_test_events(events, || frame(dialog, ctx));
    }

    fn frame_with_viewer_events(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        super::super::render::with_immediate_viewport_test_events_for(
            viewer_viewport_id(),
            events,
            || frame(dialog, ctx),
        );
    }

    fn send_child_key(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) {
        for pressed in [true, false] {
            // The Immediate renderer seam starts each child RawInput with no
            // modifiers, so model the native modifier transitions explicitly.
            let mut events = vec![egui::Event::ModifiersChanged(modifiers)];
            events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
            if !pressed {
                events.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
            }
            frame_with_child_events(dialog, ctx, events);
        }
    }

    fn send_viewer_key(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) {
        for pressed in [true, false] {
            let mut events = vec![egui::Event::ModifiersChanged(modifiers)];
            events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
            if !pressed {
                events.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
            }
            frame_with_viewer_events(dialog, ctx, events);
        }
    }

    fn click_viewer_canvas(dialog: &mut DiagramStudioDialog, ctx: &egui::Context) {
        frame(dialog, ctx);
        let click = dialog
            .test_viewer_canvas_rect
            .expect("viewer canvas has rendered")
            .center();
        for pressed in [true, false] {
            frame_with_viewer_events(
                dialog,
                ctx,
                vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        frame(dialog, ctx);
    }

    fn embedded_frame_with_events(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        root_input_allowed: bool,
    ) {
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                TEST_VIEWPORT_SIZE,
            )),
            events,
            focused: true,
            ..Default::default()
        };
        let output = ctx.run_ui(raw_input, |ui| {
            let first_pass = ui.ctx().current_pass_index() == 0;
            dialog.show(ui.ctx(), first_pass, true, root_input_allowed);
        });
        output.drop_without_applying_deltas();
        dialog.on_ui_frame_complete(ctx);
    }

    fn click_action_in_embedded_fallback(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        name: &str,
    ) {
        embedded_frame_with_events(dialog, ctx, Vec::new(), true);
        let click = action_center(dialog, name);
        for pressed in [true, false] {
            embedded_frame_with_events(
                dialog,
                ctx,
                vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                true,
            );
        }
    }

    fn send_embedded_fallback_key(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) {
        for pressed in [true, false] {
            let mut events = vec![egui::Event::ModifiersChanged(modifiers)];
            events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
            if !pressed {
                events.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
            }
            embedded_frame_with_events(dialog, ctx, events, true);
        }
    }

    fn frame_with_root_discard(dialog: &mut DiagramStudioDialog, ctx: &egui::Context) {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    TEST_VIEWPORT_SIZE,
                )),
                focused: true,
                ..Default::default()
            };
            let output = ctx.run_ui(raw_input, |ui| {
                let first_pass = ui.ctx().current_pass_index() == 0;
                dialog.show(ui.ctx(), first_pass, true, true);
                if first_pass {
                    ui.ctx()
                        .request_discard("Diagram Studio root pass test discard");
                }
            });
            output.drop_without_applying_deltas();
            dialog.on_ui_frame_complete(ctx);
        });
    }

    fn frame_with_viewport_commands_and_root_discard(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        target_viewport: egui::ViewportId,
    ) -> Vec<egui::ViewportCommand> {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    TEST_VIEWPORT_SIZE,
                )),
                focused: true,
                ..Default::default()
            };
            let output = ctx.run_ui(raw_input, |ui| {
                let first_pass = ui.ctx().current_pass_index() == 0;
                dialog.show(ui.ctx(), first_pass, true, true);
                if first_pass {
                    ui.ctx()
                        .request_discard("Diagram Studio viewer root pass test discard");
                }
            });
            let commands = output
                .viewport_output
                .get(&target_viewport)
                .map(|output| output.commands.clone())
                .unwrap_or_default();
            output.drop_without_applying_deltas();
            dialog.on_ui_frame_complete(ctx);
            commands
        })
    }

    fn viewer_close_request_commands(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
    ) -> Vec<egui::ViewportCommand> {
        super::super::render::with_immediate_viewport_test_close_request(
            viewer_viewport_id(),
            || frame_with_viewport_commands_for(dialog, ctx, viewer_viewport_id()),
        )
    }

    fn viewer_close_request_commands_with_root_discard(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
    ) -> Vec<egui::ViewportCommand> {
        super::super::render::with_immediate_viewport_test_close_request(
            viewer_viewport_id(),
            || frame_with_viewport_commands_and_root_discard(dialog, ctx, viewer_viewport_id()),
        )
    }

    fn open_viewer(dialog: &mut DiagramStudioDialog, ctx: &egui::Context) {
        click_action(dialog, ctx, "open_viewer");
        assert!(
            dialog.viewer_open,
            "accepted Open viewer request is applied once"
        );
        frame(dialog, ctx);
    }

    fn frame_with_events_and_root_discard(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        super::super::render::with_immediate_viewport_test_events(events, || {
            frame_with_root_discard(dialog, ctx)
        });
    }

    fn initialized_dialog() -> DiagramStudioDialog {
        let mut dialog = DiagramStudioDialog::default();
        dialog.test_replace_source("flowchart TD\n    A[\"Alpha\"]");
        dialog.open();
        dialog
    }

    fn initialized_visual_dialog() -> DiagramStudioDialog {
        let mut dialog = DiagramStudioDialog::default();
        assert_eq!(
            dialog.documents.request_new_visual().unwrap(),
            TransitionOutcome::Applied
        );
        dialog.reset_document_presentation();
        dialog.open();
        dialog
    }

    fn save_test_diagram(path: &Path, source: &str) -> Vec<u8> {
        let mut document = DiagramDocumentController::new();
        document.replace_source(source).unwrap();
        document.save_as(path).unwrap();
        std::fs::read(path).unwrap()
    }

    fn add_visual_layout_objects(dialog: &mut DiagramStudioDialog) -> Vec<VisualObjectId> {
        let mut operations = Vec::new();
        if let Some(mut title) = dialog
            .documents
            .visual_scene()
            .and_then(|scene| {
                scene
                    .objects
                    .iter()
                    .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
            })
            .cloned()
        {
            title.transform.x = 1_000.0;
            title.transform.y = 1_000.0;
            operations.push(VisualOperation::ReplaceObject(title));
        }
        for transform in [
            VisualTransform {
                x: 0.0,
                y: -10.0,
                width: 20.0,
                height: 10.0,
                rotation_degrees: 15.0,
            },
            VisualTransform {
                x: 40.0,
                y: 0.0,
                width: 10.0,
                height: 20.0,
                rotation_degrees: 0.0,
            },
            VisualTransform {
                x: 100.0,
                y: 10.0,
                width: 30.0,
                height: 8.0,
                rotation_degrees: 0.0,
            },
        ] {
            operations.push(VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform,
                style: VisualStyle::default(),
                kind: VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Rectangle,
                },
            }));
        }
        let outcome = dialog
            .documents
            .apply_visual_transaction(&operations)
            .expect("visual layout fixture applies atomically");
        dialog.visual_selected_objects = outcome.created_objects.iter().copied().collect();
        dialog.visual_selection = outcome.created_objects.first().copied();
        outcome.created_objects
    }

    fn add_visual_snap_objects(
        dialog: &mut DiagramStudioDialog,
    ) -> (VisualObjectId, VisualObjectId) {
        let mut title = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
            .unwrap()
            .clone();
        title.transform.x = 1_000.0;
        title.transform.y = 1_000.0;
        let outcome = dialog
            .documents
            .apply_visual_transaction(&[
                VisualOperation::ReplaceObject(title),
                VisualOperation::AddObject(VisualObjectDraft {
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: 0.0,
                        y: 0.0,
                        width: 20.0,
                        height: 20.0,
                        rotation_degrees: 0.0,
                    },
                    style: VisualStyle::default(),
                    kind: VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                }),
                VisualOperation::AddObject(VisualObjectDraft {
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: 50.0,
                        y: 100.0,
                        width: 10.0,
                        height: 10.0,
                        rotation_degrees: 0.0,
                    },
                    style: VisualStyle::default(),
                    kind: VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Ellipse,
                    },
                }),
            ])
            .expect("snap fixture scene applies atomically");
        let selected = outcome.created_objects[0];
        let target = outcome.created_objects[1];
        dialog.visual_selected_objects = [selected].into_iter().collect();
        dialog.visual_selection = Some(selected);
        dialog.camera = Camera {
            center: ScenePoint::default(),
            zoom: 1.0,
            fit_next: false,
        };
        (selected, target)
    }

    #[test]
    fn diagram_studio_visual_canvas_pointer_drag_moves_selection_once_and_supports_undo_redo() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let initial = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
            .unwrap()
            .transform;
        let revision = dialog.documents.visual_revision().unwrap();
        let start = visual_canvas_world_point(&dialog, ScenePoint::default());
        let end = start + egui::vec2(36.0, -18.0);

        drag_visual_canvas(&mut dialog, &ctx, start, end);

        let moved = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
            .unwrap()
            .transform;
        let raw_x = f64::from(end.x - start.x) / f64::from(dialog.camera.zoom);
        let raw_y = f64::from(end.y - start.y) / f64::from(dialog.camera.zoom);
        let spacing = crate::diagram::VISUAL_LAYOUT_GRID_SPACING;
        let snap_from_min = |min: f64, raw: f64| ((min + raw) / spacing).round() * spacing - min;
        let expected_x = snap_from_min(initial.x - initial.width / 2.0, raw_x);
        let expected_y = snap_from_min(initial.y - initial.height / 2.0, raw_y);
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 1));
        assert!(dialog.documents.can_undo());
        assert!((moved.x - initial.x - expected_x).abs() < 0.05);
        assert!((moved.y - initial.y - expected_y).abs() < 0.05);

        click_action(&mut dialog, &ctx, "visual_undo");
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 2));
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .objects
                .iter()
                .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
                .unwrap()
                .transform,
            initial
        );
        click_action(&mut dialog, &ctx, "visual_redo");
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 3));
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .objects
                .iter()
                .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
                .unwrap()
                .transform,
            moved
        );
    }

    #[test]
    fn diagram_studio_visual_canvas_escape_cancels_owned_gesture_before_close() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.documents.visual_revision().unwrap();
        let start = visual_canvas_world_point(&dialog, ScenePoint::default());
        let end = start + egui::vec2(40.0, 12.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        frame_with_child_events(&mut dialog, &ctx, vec![egui::Event::PointerMoved(end)]);
        assert!(
            dialog
                .visual_gesture
                .as_ref()
                .is_some_and(|gesture| gesture.start_pass_accepted)
        );

        send_child_key(&mut dialog, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(end), button(end, false)],
        );

        assert!(dialog.open, "first Escape cancels the canvas gesture");
        assert!(dialog.visual_gesture.is_none());
        assert_eq!(dialog.documents.visual_revision(), Some(revision));
        assert!(!dialog.documents.can_undo());
    }

    #[test]
    fn diagram_studio_visual_canvas_discarded_release_keeps_document_unchanged() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.documents.visual_revision().unwrap();
        let start = visual_canvas_world_point(&dialog, ScenePoint::default());
        let end = start + egui::vec2(40.0, 16.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        frame_with_child_events(&mut dialog, &ctx, vec![egui::Event::PointerMoved(end)]);
        assert!(dialog.visual_gesture.is_some());
        dialog.test_request_child_discard();
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(end), button(end, false)],
        );

        assert_eq!(dialog.documents.visual_revision(), Some(revision));
        assert!(!dialog.documents.can_undo());
        assert!(dialog.visual_gesture.is_none());
    }

    #[test]
    fn diagram_studio_visual_canvas_rejects_release_from_retired_owner_generation() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.documents.visual_revision().unwrap();
        let start = visual_canvas_world_point(&dialog, ScenePoint::default());
        let end = start + egui::vec2(34.0, 18.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        frame_with_child_events(&mut dialog, &ctx, vec![egui::Event::PointerMoved(end)]);
        dialog.owner_generation = dialog.owner_generation.wrapping_add(1);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(end), button(end, false)],
        );

        assert_eq!(dialog.documents.visual_revision(), Some(revision));
        assert!(!dialog.documents.can_undo());
        assert!(dialog.visual_gesture.is_none());
    }

    #[test]
    fn diagram_studio_visual_canvas_text_focus_blocks_pointer_transform() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        click_action(&mut dialog, &ctx, "visual_label");
        assert!(dialog.test_action_has_focus("visual_label"));
        let revision = dialog.documents.visual_revision().unwrap();
        let original_x = dialog.documents.visual_scene().unwrap().objects[0]
            .transform
            .x;
        let start = visual_canvas_world_point(&dialog, ScenePoint::default());
        let end = start + egui::vec2(42.0, 15.0);
        drag_visual_canvas(&mut dialog, &ctx, start, end);

        assert_eq!(dialog.documents.visual_revision(), Some(revision));
        assert_eq!(
            dialog.documents.visual_scene().unwrap().objects[0]
                .transform
                .x,
            original_x
        );
        assert!(dialog.visual_gesture.is_none());
    }

    #[test]
    fn diagram_studio_visual_canvas_multiselect_drag_is_one_group_transaction() {
        let mut dialog = initialized_visual_dialog();
        let shape_id = dialog
            .documents
            .apply_visual_transaction(&[VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform {
                    x: 170.0,
                    y: 85.0,
                    width: 70.0,
                    height: 46.0,
                    ..VisualTransform::default()
                },
                style: VisualStyle::default(),
                kind: VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Rectangle,
                },
            })])
            .unwrap()
            .created_objects[0];
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let shape_center = visual_canvas_world_point(&dialog, ScenePoint { x: 170.0, y: 85.0 });
        for pressed in [true, false] {
            let modifiers = egui::Modifiers::SHIFT;
            frame_with_child_events(
                &mut dialog,
                &ctx,
                vec![
                    egui::Event::ModifiersChanged(modifiers),
                    egui::Event::PointerMoved(shape_center),
                    egui::Event::PointerButton {
                        pos: shape_center,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers,
                    },
                ],
            );
        }
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
        );
        assert!(
            dialog
                .visual_selected_objects
                .contains(&crate::diagram::TITLE_OBJECT_ID)
        );
        assert!(dialog.visual_selected_objects.contains(&shape_id));
        let before_title_x = dialog.documents.visual_scene().unwrap().objects[0]
            .transform
            .x;
        let before_shape_x = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == shape_id)
            .unwrap()
            .transform
            .x;
        let revision = dialog.documents.visual_revision().unwrap();
        let end = shape_center + egui::vec2(30.0, 12.0);
        drag_visual_canvas(&mut dialog, &ctx, shape_center, end);

        let scene = dialog.documents.visual_scene().unwrap();
        let after_title_x = scene.objects[0].transform.x;
        let after_shape_x = scene
            .objects
            .iter()
            .find(|object| object.id == shape_id)
            .unwrap()
            .transform
            .x;
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 1));
        assert!((after_title_x - before_title_x - after_shape_x + before_shape_x).abs() < 1e-9);
        assert!(after_shape_x > before_shape_x);
        assert_eq!(dialog.visual_selected_objects.len(), 2);
    }

    #[test]
    fn diagram_studio_visual_container_controls_membership_and_group_drag_once() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        click_action(&mut dialog, &ctx, "visual_add_container");
        click_action(&mut dialog, &ctx, "visual_add_container");
        let frame_ids = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .filter_map(|object| {
                matches!(&object.kind, VisualObjectKind::Container { .. }).then_some(object.id)
            })
            .collect::<Vec<_>>();
        assert_eq!(frame_ids.len(), 2, "the frame control creates containers");
        let frame_id = frame_ids[0];
        let other_frame_id = frame_ids[1];
        click_action(&mut dialog, &ctx, "visual_add_shape");
        let child_id = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find_map(|object| {
                matches!(&object.kind, VisualObjectKind::Primitive { .. }).then_some(object.id)
            })
            .expect("the shape control creates a member candidate");

        let mut child = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == child_id)
            .unwrap()
            .clone();
        let frame_object = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == frame_id)
            .unwrap();
        let mut other_frame = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == other_frame_id)
            .unwrap()
            .clone();
        other_frame.transform.x = 400.0;
        other_frame.transform.y = 0.0;
        child.transform.x = frame_object.transform.x;
        child.transform.y = frame_object.transform.y;
        child.transform.width = 80.0;
        child.transform.height = 50.0;
        dialog
            .documents
            .apply_visual_transaction(&[
                VisualOperation::ReplaceObject(child),
                VisualOperation::ReplaceObject(other_frame),
                VisualOperation::AddObject(VisualObjectDraft {
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: 120.0,
                        y: 0.0,
                        width: 70.0,
                        height: 50.0,
                        ..VisualTransform::default()
                    },
                    style: VisualStyle::default(),
                    kind: VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Ellipse,
                    },
                }),
            ])
            .unwrap();
        dialog.revalidate_visual_selection();
        let external_id = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find_map(|object| {
                matches!(
                    &object.kind,
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Ellipse
                    }
                )
                .then_some(object.id)
            })
            .unwrap();
        let connector_id = dialog
            .documents
            .apply_visual_transaction(&[VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform::default(),
                style: VisualStyle::default(),
                kind: VisualObjectKind::Connector {
                    connector: VisualConnector {
                        from: VisualConnectorEndpoint {
                            object_id: child_id,
                            anchor: VisualAnchor::East,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: external_id,
                            anchor: VisualAnchor::West,
                        },
                        route: VisualConnectorRoute::Straight,
                        direction: VisualConnectorDirection::None,
                        label: String::new(),
                    },
                },
            })])
            .unwrap()
            .created_objects[0];

        let mixed = [crate::diagram::TITLE_OBJECT_ID, child_id, connector_id].into_iter().collect();
        assert_eq!(visual_group_roots(dialog.documents.visual_scene().unwrap(), &mixed), vec![child_id]);
        let ineligible = [crate::diagram::TITLE_OBJECT_ID, connector_id].into_iter().collect();
        assert!(visual_group_roots(dialog.documents.visual_scene().unwrap(), &ineligible).is_empty());

        click_action(&mut dialog, &ctx, "visual_object_selection");
        click_action(
            &mut dialog,
            &ctx,
            &format!("visual_object_option_{}", child_id.get()),
        );
        click_action(&mut dialog, &ctx, "visual_membership_parent");
        click_action(
            &mut dialog,
            &ctx,
            &format!("visual_parent_option_{}", frame_id.get()),
        );
        click_action(&mut dialog, &ctx, "visual_assign_parent");
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .parent_by_child
                .get(&child_id),
            Some(&frame_id)
        );
        // Verify the working parent controls can reparent and detach as well as
        // make the final membership used by the group gesture below.
        click_action(&mut dialog, &ctx, "visual_membership_parent");
        click_action(
            &mut dialog,
            &ctx,
            &format!("visual_parent_option_{}", other_frame_id.get()),
        );
        click_action(&mut dialog, &ctx, "visual_assign_parent");
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .parent_by_child
                .get(&child_id),
            Some(&other_frame_id),
            "Assign / reparent uses the accepted visual transaction"
        );
        click_action(&mut dialog, &ctx, "visual_detach_parent");
        assert!(
            !dialog
                .documents
                .visual_scene()
                .unwrap()
                .parent_by_child
                .contains_key(&child_id)
        );
        click_action(&mut dialog, &ctx, "visual_membership_parent");
        click_action(
            &mut dialog,
            &ctx,
            &format!("visual_parent_option_{}", frame_id.get()),
        );
        click_action(&mut dialog, &ctx, "visual_assign_parent");
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .parent_by_child
                .get(&child_id),
            Some(&frame_id)
        );

        click_action(&mut dialog, &ctx, "visual_object_selection");
        click_action(
            &mut dialog,
            &ctx,
            &format!("visual_object_option_{}", frame_id.get()),
        );
        for _ in 0..2 {
            frame(&mut dialog, &ctx);
        }
        let transform_for = |dialog: &DiagramStudioDialog, id| {
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .objects
                .iter()
                .find(|object| object.id == id)
                .unwrap()
                .transform
        };
        let frame_before = transform_for(&dialog, frame_id);
        let child_before = transform_for(&dialog, child_id);
        let scene_before_gesture = dialog.documents.visual_scene().unwrap().clone();
        let path_for = |scene: &VisualScene| {
            project_visual_scene(scene)
                .unwrap()
                .items
                .into_iter()
                .find(|item| item.object_id == connector_id)
                .unwrap()
                .open_paths
        };
        let route_before = path_for(&scene_before_gesture);
        click_action(&mut dialog, &ctx, "visual_fit");
        for _ in 0..2 {
            frame(&mut dialog, &ctx);
        }
        let frame_header = visual_canvas_world_point(
            &dialog,
            ScenePoint {
                x: frame_before.x,
                y: frame_before.y - frame_before.height / 2.0 + 12.0,
            },
        );
        let fit_scene = dialog.documents.visual_scene().unwrap();
        let fit_projection = project_visual_scene(fit_scene).unwrap();
        let fit_mapping = pure_canvas_mapping(
            dialog.test_visual_canvas_rect.unwrap().shrink(12.0),
            dialog.camera,
        );
        let fit_point = scene_point_from_screen(frame_header);
        assert!(
            fit_mapping.contains_screen_point(fit_point),
            "frame header outside fitted clip: mapping={fit_mapping:?}, point={fit_point:?}"
        );
        assert!(
            visual_painted_item_prefix(&fit_projection)
                > fit_projection
                    .items
                    .iter()
                    .position(|item| item.object_id == frame_id)
                    .unwrap()
        );
        assert_eq!(
            visual_canvas_hit_test(
                &fit_projection,
                visual_painted_item_prefix(&fit_projection),
                &[],
                fit_mapping,
                fit_point,
                9.0
            ),
            CanvasHit::Object(frame_id)
        );
        let button = |pos, pressed, modifiers| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        };
        for pressed in [true, false] {
            frame_with_child_events(
                &mut dialog,
                &ctx,
                vec![
                    egui::Event::PointerMoved(frame_header),
                    button(frame_header, pressed, egui::Modifiers::NONE),
                ],
            );
        }
        assert_eq!(dialog.visual_selection, Some(frame_id));
        for _ in 0..2 {
            frame(&mut dialog, &ctx);
        }
        let child_center = visual_canvas_world_point(
            &dialog,
            ScenePoint {
                x: child_before.x - child_before.width / 4.0,
                y: child_before.y + child_before.height / 4.0,
            },
        );
        let scene = dialog.documents.visual_scene().unwrap();
        let projection = project_visual_scene(scene).unwrap();
        let canvas = dialog.test_visual_canvas_rect.unwrap();
        let mapping = pure_canvas_mapping(canvas.shrink(12.0), dialog.camera);
        let child_screen_point = scene_point_from_screen(child_center);
        assert!(
            mapping.contains_screen_point(child_screen_point),
            "child outside current clip: mapping={mapping:?}, camera={:?}, canvas={canvas:?}, child_transform={child_before:?}, child_screen={child_screen_point:?}, hit_layer={:?}",
            dialog.camera,
            ctx.layer_id_at(child_center)
        );
        let child_world_point = mapping.screen_to_world(child_screen_point);
        let projected_child = projection
            .items
            .iter()
            .find(|item| item.object_id == child_id)
            .unwrap();
        assert!(projected_child.bounds.contains_point(child_world_point));
        assert!(
            visual_painted_item_prefix(&projection)
                > projection
                    .items
                    .iter()
                    .position(|item| item.object_id == child_id)
                    .unwrap()
        );
        let handles = visual_canvas_selection_handles(
            scene,
            &projection,
            &dialog.visual_selected_objects,
            visual_painted_item_prefix(&projection),
            mapping,
        );
        assert_eq!(
            visual_canvas_hit_test(
                &projection,
                visual_painted_item_prefix(&projection),
                &handles,
                mapping,
                child_screen_point,
                9.0,
            ),
            CanvasHit::Object(child_id),
            "the modifier click point resolves to the visible child outline"
        );
        for pressed in [true, false] {
            let modifiers = egui::Modifiers::SHIFT;
            frame_with_child_events(
                &mut dialog,
                &ctx,
                vec![
                    egui::Event::ModifiersChanged(modifiers),
                    egui::Event::PointerMoved(child_center),
                    button(child_center, pressed, modifiers),
                ],
            );
        }
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
        );
        assert_eq!(
            dialog.visual_selected_objects,
            [frame_id, child_id].into_iter().collect()
        );
        assert_eq!(dialog.visual_selection, Some(child_id));
        let revision = dialog.documents.visual_revision().unwrap();
        let start = visual_canvas_world_point(
            &dialog,
            ScenePoint {
                x: frame_before.x,
                y: frame_before.y - frame_before.height / 2.0 + 12.0,
            },
        );
        let end = start + egui::vec2(32.0, 18.0);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                button(start, true, egui::Modifiers::NONE),
            ],
        );
        frame_with_child_events(&mut dialog, &ctx, vec![egui::Event::PointerMoved(end)]);
        let active = dialog
            .visual_gesture
            .as_ref()
            .expect("the selected frame owns a group gesture");
        let preview_path = active
            .preview_projection
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|item| item.object_id == connector_id)
            .unwrap()
            .open_paths
            .clone();
        assert_ne!(
            preview_path, route_before,
            "the linked path follows the previewed child"
        );
        assert_eq!(dialog.documents.visual_revision(), Some(revision));
        assert_eq!(
            path_for(dialog.documents.visual_scene().unwrap()),
            route_before,
            "preview routing does not mutate the committed document"
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(end),
                button(end, false, egui::Modifiers::NONE),
            ],
        );
        frame(&mut dialog, &ctx);

        let frame_after = transform_for(&dialog, frame_id);
        let child_after = transform_for(&dialog, child_id);
        let frame_delta = ScenePoint {
            x: frame_after.x - frame_before.x,
            y: frame_after.y - frame_before.y,
        };
        let child_delta = ScenePoint {
            x: child_after.x - child_before.x,
            y: child_after.y - child_before.y,
        };
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 1));
        assert_eq!(
            path_for(dialog.documents.visual_scene().unwrap()),
            preview_path,
            "the accepted connector route matches the preview"
        );
        assert_eq!(dialog.visual_selected_objects.len(), 2);
        assert_eq!(dialog.visual_selection, Some(child_id));
        assert!(frame_delta.x.abs() > 1.0 || frame_delta.y.abs() > 1.0);
        assert!((frame_delta.x - child_delta.x).abs() < 1e-9);
        assert!((frame_delta.y - child_delta.y).abs() < 1e-9);

        click_action(&mut dialog, &ctx, "visual_undo");
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .parent_by_child
                .get(&child_id),
            Some(&frame_id)
        );
        assert_eq!(transform_for(&dialog, frame_id), frame_before);
        assert_eq!(transform_for(&dialog, child_id), child_before);
        assert_eq!(
            path_for(dialog.documents.visual_scene().unwrap()),
            route_before
        );
        assert_eq!(dialog.visual_selected_objects.len(), 2);
        assert_eq!(dialog.visual_selection, Some(child_id));

        let before_discarded_release = dialog.documents.visual_scene().unwrap().clone();
        let discarded_revision = dialog.documents.visual_revision().unwrap();
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                button(start, true, egui::Modifiers::NONE),
            ],
        );
        frame_with_child_events(&mut dialog, &ctx, vec![egui::Event::PointerMoved(end)]);
        assert_ne!(
            dialog
                .visual_gesture
                .as_ref()
                .unwrap()
                .preview_projection
                .as_ref()
                .unwrap()
                .items
                .iter()
                .find(|item| item.object_id == connector_id)
                .unwrap()
                .open_paths,
            route_before
        );
        dialog.test_request_child_discard();
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(end),
                button(end, false, egui::Modifiers::NONE),
            ],
        );
        assert_eq!(dialog.documents.visual_revision(), Some(discarded_revision));
        assert_eq!(
            dialog.documents.visual_scene().unwrap(),
            &before_discarded_release
        );
        assert!(dialog.visual_gesture.is_none());
    }

    #[test]
    fn diagram_studio_visual_group_and_ungroup_controls_are_atomic_and_retained() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        click_action(&mut dialog, &ctx, "visual_add_shape");
        let child = dialog.visual_selection.unwrap();
        click_action(&mut dialog, &ctx, "visual_object_selection");
        click_action(
            &mut dialog,
            &ctx,
            &format!(
                "visual_object_option_{}",
                crate::diagram::TITLE_OBJECT_ID.get()
            ),
        );
        frame(&mut dialog, &ctx);
        assert!(
            !dialog.test_visual_group_enabled,
            "the title alone cannot be grouped"
        );
        click_action(&mut dialog, &ctx, "visual_object_selection");
        click_action(
            &mut dialog,
            &ctx,
            &format!("visual_object_option_{}", child.get()),
        );
        frame(&mut dialog, &ctx);
        assert!(dialog.test_visual_group_enabled);
        let before = dialog.documents.visual_scene().unwrap().clone();
        let revision = dialog.documents.visual_revision().unwrap();
        click_action_with_child_discard(&mut dialog, &ctx, "visual_group_selection");
        assert_eq!(dialog.documents.visual_scene().unwrap(), &before);
        assert_eq!(dialog.documents.visual_revision(), Some(revision));
        click_action_with_root_discard(&mut dialog, &ctx, "visual_group_selection");
        assert_eq!(dialog.documents.visual_scene().unwrap(), &before);
        assert_eq!(dialog.documents.visual_revision(), Some(revision));
        click_action(&mut dialog, &ctx, "visual_group_selection");
        let frame_id = dialog.visual_selection.unwrap();
        assert_ne!(frame_id, child);
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 1));
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .parent_by_child
                .get(&child),
            Some(&frame_id)
        );
        let grouped = dialog.documents.visual_scene().unwrap().clone();
        click_action_with_child_discard(&mut dialog, &ctx, "visual_ungroup");
        assert_eq!(dialog.documents.visual_scene().unwrap(), &grouped);
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 1));
        click_action(&mut dialog, &ctx, "visual_ungroup");
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 2));
        assert_eq!(dialog.documents.visual_scene().unwrap(), &before);
        click_action(&mut dialog, &ctx, "visual_undo");
        assert_eq!(dialog.documents.visual_scene().unwrap(), &grouped);
    }

    #[test]
    fn diagram_studio_visual_container_controls_create_and_edit_swimlane() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        click_action(&mut dialog, &ctx, "visual_add_horizontal_swimlane");
        let lane_id = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find_map(|object| {
                matches!(
                    &object.kind,
                    VisualObjectKind::Swimlane {
                        orientation: VisualSwimlaneOrientation::Horizontal,
                        ..
                    }
                )
                .then_some(object.id)
            })
            .expect("the lane control creates a horizontal swimlane");

        click_action(&mut dialog, &ctx, "visual_frame_label");
        send_child_key(&mut dialog, &ctx, egui::Key::A, egui::Modifiers::COMMAND);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::Text("Renamed lane".into())],
        );
        assert!(dialog.visual_property_draft.as_ref().is_some_and(|draft| {
            matches!(
                &draft.object.kind,
                VisualObjectKind::Swimlane {
                    orientation: VisualSwimlaneOrientation::Horizontal,
                    label,
                } if label == "Renamed lane"
            )
        }));
        click_action(&mut dialog, &ctx, "visual_swimlane_orientation");
        click_action(&mut dialog, &ctx, "visual_swimlane_vertical");
        assert!(dialog.visual_property_draft.as_ref().is_some_and(|draft| {
            matches!(
                &draft.object.kind,
                VisualObjectKind::Swimlane {
                    orientation: VisualSwimlaneOrientation::Vertical,
                    label,
                } if label == "Renamed lane"
            )
        }));
        let before_discard = dialog.documents.visual_scene().unwrap().clone();
        let before_revision = dialog.documents.visual_revision();
        reveal_visual_action(&mut dialog, &ctx, "visual_apply_properties");
        click_action_with_child_discard(&mut dialog, &ctx, "visual_apply_properties");
        assert_eq!(dialog.documents.visual_scene().unwrap(), &before_discard);
        assert_eq!(dialog.documents.visual_revision(), before_revision);
        click_action(&mut dialog, &ctx, "visual_apply_properties");

        let lane = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == lane_id)
            .unwrap();
        assert!(matches!(
            &lane.kind,
            VisualObjectKind::Swimlane {
                orientation: VisualSwimlaneOrientation::Vertical,
                label,
            } if label == "Renamed lane"
        ));
    }

    #[test]
    fn diagram_studio_visual_layout_snap_preview_matches_release_and_cancel_clears_guides() {
        let mut dialog = initialized_visual_dialog();
        let (selected_id, target_id) = add_visual_snap_objects(&mut dialog);
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.documents.visual_revision().unwrap();
        let before = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == selected_id)
            .unwrap()
            .clone();
        let start = visual_canvas_world_point(&dialog, ScenePoint::default());
        let end = visual_canvas_world_point(&dialog, ScenePoint { x: 34.0, y: 0.0 });
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        frame_with_child_events(&mut dialog, &ctx, vec![egui::Event::PointerMoved(end)]);
        let active = dialog
            .visual_gesture
            .as_ref()
            .expect("drag owns a live visual gesture");
        assert_eq!(active.preview_guides.len(), 1);
        assert_eq!(active.preview_guides[0].target_object, target_id);
        assert_eq!(active.preview_guides[0].axis, CanvasGuideAxis::Vertical);
        let preview = active
            .preview_objects
            .iter()
            .find(|object| object.id == selected_id)
            .unwrap()
            .clone();
        assert!((preview.transform.x - before.transform.x - 35.0).abs() < 1e-8);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(end), button(end, false)],
        );
        frame(&mut dialog, &ctx);
        let committed = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == selected_id)
            .unwrap();
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 1));
        assert_eq!(committed.transform, preview.transform);

        let mut cancelled = initialized_visual_dialog();
        let (selected_id, _) = add_visual_snap_objects(&mut cancelled);
        let cancel_ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut cancelled, &cancel_ctx);
        }
        let cancel_revision = cancelled.documents.visual_revision().unwrap();
        let cancel_before = cancelled.documents.visual_scene().unwrap().clone();
        let start = visual_canvas_world_point(&cancelled, ScenePoint::default());
        let end = visual_canvas_world_point(&cancelled, ScenePoint { x: 34.0, y: 0.0 });
        frame_with_child_events(
            &mut cancelled,
            &cancel_ctx,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        frame_with_child_events(
            &mut cancelled,
            &cancel_ctx,
            vec![egui::Event::PointerMoved(end)],
        );
        assert!(
            cancelled
                .visual_gesture
                .as_ref()
                .is_some_and(|gesture| !gesture.preview_guides.is_empty())
        );
        send_child_key(
            &mut cancelled,
            &cancel_ctx,
            egui::Key::Escape,
            egui::Modifiers::NONE,
        );
        frame_with_child_events(
            &mut cancelled,
            &cancel_ctx,
            vec![egui::Event::PointerMoved(end), button(end, false)],
        );
        assert!(cancelled.visual_gesture.is_none());
        assert_eq!(cancelled.documents.visual_revision(), Some(cancel_revision));
        assert_eq!(cancelled.documents.visual_scene().unwrap(), &cancel_before);
        assert_eq!(cancelled.visual_selection, Some(selected_id));
    }

    #[test]
    fn diagram_studio_visual_layout_align_and_distribute_are_single_undo_steps() {
        let mut dialog = initialized_visual_dialog();
        let selected = add_visual_layout_objects(&mut dialog);
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let before = dialog.documents.visual_scene().unwrap().clone();
        let initial_revision = dialog.documents.visual_revision().unwrap();

        click_action(&mut dialog, &ctx, "visual_align_top");
        assert_eq!(
            dialog.documents.visual_revision(),
            Some(initial_revision + 1)
        );
        assert_eq!(
            dialog.visual_selected_objects,
            selected.iter().copied().collect()
        );
        let aligned_projection =
            project_visual_scene(dialog.documents.visual_scene().unwrap()).unwrap();
        let aligned_tops = selected
            .iter()
            .map(|id| {
                visual_canvas_selection_bounds(
                    dialog.documents.visual_scene().unwrap(),
                    &aligned_projection,
                    &[*id].into_iter().collect(),
                    aligned_projection.items.len(),
                )
                .unwrap()
                .min
                .y
            })
            .collect::<Vec<_>>();
        assert!(
            aligned_tops
                .iter()
                .all(|top| (top - aligned_tops[0]).abs() < 1e-8)
        );
        click_action(&mut dialog, &ctx, "visual_undo");
        assert_eq!(
            dialog.documents.visual_revision(),
            Some(initial_revision + 2)
        );
        assert_eq!(dialog.documents.visual_scene().unwrap(), &before);
        assert_eq!(
            dialog.visual_selected_objects,
            selected.iter().copied().collect()
        );

        click_action(&mut dialog, &ctx, "visual_distribute_horizontal");
        assert_eq!(
            dialog.documents.visual_revision(),
            Some(initial_revision + 3)
        );
        assert_eq!(
            dialog.visual_selected_objects,
            selected.iter().copied().collect()
        );
        let distributed_projection =
            project_visual_scene(dialog.documents.visual_scene().unwrap()).unwrap();
        let bounds = selected
            .iter()
            .map(|id| {
                visual_canvas_selection_bounds(
                    dialog.documents.visual_scene().unwrap(),
                    &distributed_projection,
                    &[*id].into_iter().collect(),
                    distributed_projection.items.len(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let first_gap = bounds[1].min.x - bounds[0].max.x;
        let second_gap = bounds[2].min.x - bounds[1].max.x;
        assert!((first_gap - second_gap).abs() < 1e-8);
        click_action(&mut dialog, &ctx, "visual_undo");
        assert_eq!(
            dialog.documents.visual_revision(),
            Some(initial_revision + 4)
        );
        assert_eq!(dialog.documents.visual_scene().unwrap(), &before);
        assert_eq!(
            dialog.visual_selected_objects,
            selected.iter().copied().collect()
        );
    }

    #[test]
    fn visual_marquee_replaces_then_command_adds_without_history() {
        let mut dialog = initialized_visual_dialog();
        let (first, second) = add_visual_snap_objects(&mut dialog);
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.documents.visual_revision();
        let drag = |dialog: &mut DiagramStudioDialog,
                        from: ScenePoint,
                        to: ScenePoint,
                        modifiers: egui::Modifiers| {
            let start = visual_canvas_world_point(dialog, from);
            let end = visual_canvas_world_point(dialog, to);
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            };
            frame_with_child_events(
                dialog,
                &ctx,
                vec![
                    egui::Event::ModifiersChanged(modifiers),
                    egui::Event::PointerMoved(start),
                    button(start, true),
                ],
            );
            frame_with_child_events(
                dialog,
                &ctx,
                vec![
                    egui::Event::ModifiersChanged(modifiers),
                    egui::Event::PointerMoved(end),
                ],
            );
            frame_with_child_events(
                dialog,
                &ctx,
                vec![egui::Event::ModifiersChanged(modifiers), button(end, false)],
            );
        };
        drag(
            &mut dialog,
            ScenePoint { x: -20.0, y: -20.0 },
            ScenePoint { x: 20.0, y: 20.0 },
            egui::Modifiers::NONE,
        );
        assert_eq!(
            dialog.visual_selected_objects,
            [first].into_iter().collect()
        );
        drag(
            &mut dialog,
            ScenePoint { x: 35.0, y: 85.0 },
            ScenePoint { x: 65.0, y: 115.0 },
            egui::Modifiers::COMMAND,
        );
        assert_eq!(
            dialog.visual_selected_objects,
            [first, second].into_iter().collect()
        );
        assert_eq!(dialog.documents.visual_revision(), revision);
    }

    #[test]
    fn visual_marquee_is_painted_above_background_and_clipped() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let canvas = dialog.test_visual_canvas_rect.unwrap();
        let start = canvas.left_top() + egui::vec2(20.0, 20.0);
        let end = start + egui::vec2(70.0, 55.0);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame_with_child_events(&mut dialog, &ctx, vec![egui::Event::PointerMoved(end)]);
        let (background, marquee, clip) = dialog
            .test_marquee_order
            .expect("actual marquee and scene background were emitted");
        assert!(marquee > background);
        assert!(canvas.contains_rect(clip));
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }

    #[test]
    fn visual_minimap_drag_keeps_press_mapping_and_camera_independence() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        open_viewer(&mut dialog, &ctx);
        frame(&mut dialog, &ctx);
        let revision = dialog.documents.visual_revision();
        let viewer_before = dialog.viewer_camera;
        let frame_rect = dialog.test_action_rect("visual_minimap").unwrap().0;
        let start = frame_rect.center();
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let drag = dialog
            .visual_editor_minimap_drag
            .as_ref()
            .expect("minimap press captures mapping")
            .clone();
        let end = start + egui::vec2(12.0, 8.0);
        frame_with_child_events(&mut dialog, &ctx, vec![egui::Event::PointerMoved(end)]);
        let world = drag
            .layout
            .screen_to_world(scene_point_from_screen(end))
            .unwrap();
        assert!((dialog.camera.center.x - (world.x + drag.offset.x)).abs() < 1e-6);
        assert!((dialog.camera.center.y - (world.y + drag.offset.y)).abs() < 1e-6);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(dialog.visual_editor_minimap_drag.is_none());
        assert_eq!(dialog.viewer_camera.center, viewer_before.center);
        assert_eq!(dialog.documents.visual_revision(), revision);

        frame(&mut dialog, &ctx);
        let editor_before = dialog.camera;
        let frame_rect = dialog.test_action_rect("visual_viewer_minimap").unwrap().0;
        let start = frame_rect.center();
        frame_with_viewer_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let drag = dialog
            .visual_viewer_minimap_drag
            .as_ref()
            .expect("viewer minimap captures mapping")
            .clone();
        let end = start + egui::vec2(9.0, 6.0);
        frame_with_viewer_events(&mut dialog, &ctx, vec![egui::Event::PointerMoved(end)]);
        let world = drag
            .layout
            .screen_to_world(scene_point_from_screen(end))
            .unwrap();
        assert!((dialog.viewer_camera.center.x - (world.x + drag.offset.x)).abs() < 1e-6);
        frame_with_viewer_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(dialog.visual_viewer_minimap_drag.is_none());
        assert_eq!(dialog.camera.center, editor_before.center);
        assert_eq!(dialog.documents.visual_revision(), revision);
    }

    #[test]
    fn visual_minimap_discarded_press_cannot_move_or_survive() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let before = dialog.camera;
        let point = dialog
            .test_action_rect("visual_minimap")
            .unwrap()
            .0
            .left_top()
            + egui::vec2(8.0, 8.0);
        dialog.test_request_child_discard();
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(dialog.visual_editor_minimap_drag.is_none());
        assert_eq!(dialog.camera.center, before.center);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        open_viewer(&mut dialog, &ctx);
        frame(&mut dialog, &ctx);
        let before = dialog.viewer_camera;
        let fit_follow = dialog.visual_viewer_fit_follow;
        let point = dialog
            .test_action_rect("visual_viewer_minimap")
            .unwrap()
            .0
            .left_top()
            + egui::vec2(8.0, 8.0);
        dialog.test_request_viewer_child_discard();
        frame_with_viewer_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(dialog.visual_viewer_minimap_drag.is_none());
        assert_eq!(dialog.viewer_camera.center, before.center);
        assert_eq!(dialog.visual_viewer_fit_follow, fit_follow);
    }

    #[test]
    fn visual_viewer_fit_tracks_resize_until_manual_zoom() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        open_viewer(&mut dialog, &ctx);
        for _ in 0..3 {
            frame_with_viewer_size(&mut dialog, &ctx, egui::vec2(700.0, 450.0));
        }
        let small = dialog.test_viewer_canvas_rect.unwrap();
        assert!(dialog.visual_viewer_fit_follow);
        for _ in 0..3 {
            frame_with_viewer_size(&mut dialog, &ctx, egui::vec2(950.0, 750.0));
        }
        let large = dialog.test_viewer_canvas_rect.unwrap();
        assert!(large.width() > small.width() && large.height() > small.height());
        assert_eq!(dialog.visual_viewer_fit_size, Some(large.size()));
        for _ in 0..3 {
            frame_with_viewer_size(&mut dialog, &ctx, egui::vec2(2_000.0, 2_000.0));
        }
        let expanded = dialog.test_viewer_canvas_rect.unwrap();
        assert!(expanded.width() > 1_400.0 && expanded.height() > 1_600.0);
        click_viewer_action(&mut dialog, &ctx, "visual_viewer_zoom_in");
        assert!(!dialog.visual_viewer_fit_follow);
        let manual = dialog.viewer_camera;
        for _ in 0..3 {
            frame_with_viewer_size(&mut dialog, &ctx, egui::vec2(750.0, 550.0));
        }
        assert_eq!(dialog.viewer_camera.center, manual.center);
        assert_eq!(dialog.viewer_camera.zoom, manual.zoom);
        click_viewer_action(&mut dialog, &ctx, "visual_viewer_fit");
        assert!(dialog.visual_viewer_fit_follow);
        click_viewer_canvas(&mut dialog, &ctx);
        dialog.viewer_camera.zoom = CAMERA_MAX_ZOOM;
        dialog.viewer_camera.fit_next = false;
        dialog.visual_viewer_fit_follow = true;
        send_viewer_key(
            &mut dialog,
            &ctx,
            egui::Key::Plus,
            egui::Modifiers::COMMAND,
        );
        assert_eq!(dialog.viewer_camera.zoom, CAMERA_MAX_ZOOM);
        assert!(
            !dialog.visual_viewer_fit_follow,
            "manual zoom at the limit disables resize-follow"
        );
    }

    #[test]
    fn visual_font_picker_preserves_unknown_family() {
        assert_eq!(visual_font_family_label("sans-serif"), "Sans serif");
        assert_eq!(visual_font_family_label("MONOSPACE"), "Monospace");
        let mut family = "Imported Font".to_owned();
        assert_eq!(
            visual_font_family_label(&family),
            "Imported Font (sans-serif fallback)"
        );
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let picker_rect = std::cell::Cell::new(egui::Rect::NOTHING);
        let run = |events: Vec<egui::Event>, family: &mut String| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 400.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    picker_rect.set(visual_font_family_picker(ui, family).rect);
                },
            );
            output.drop_without_applying_deltas();
        };
        run(vec![], &mut family);
        assert_eq!(family, "Imported Font");
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let pos = picker_rect.get().center();
        run(
            vec![egui::Event::PointerMoved(pos), button(pos, true)],
            &mut family,
        );
        run(vec![button(pos, false)], &mut family);
        run(vec![], &mut family);
        let rect = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("test-visual-font-monospace")))
            .expect("font menu is open");
        let pos = rect.center();
        run(
            vec![egui::Event::PointerMoved(pos), button(pos, true)],
            &mut family,
        );
        run(vec![button(pos, false)], &mut family);
        assert_eq!(family, "monospace");
    }

    #[test]
    fn diagram_studio_visual_layout_minimap_recenters_only_its_own_camera() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        open_viewer(&mut dialog, &ctx);
        frame(&mut dialog, &ctx);
        let revision = dialog.documents.visual_revision().unwrap();
        let root_before = dialog.camera;
        let viewer_before = dialog.viewer_camera;
        let root_minimap = dialog.test_action_rect("visual_minimap").unwrap().0;
        let root_click = root_minimap.left_top() + egui::vec2(8.0, 8.0);
        for pressed in [true, false] {
            frame_with_child_events(
                &mut dialog,
                &ctx,
                vec![
                    egui::Event::PointerMoved(root_click),
                    egui::Event::PointerButton {
                        pos: root_click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_ne!(dialog.camera.center, root_before.center);
        assert_eq!(dialog.viewer_camera.center, viewer_before.center);
        assert_eq!(dialog.viewer_camera.zoom, viewer_before.zoom);
        assert_eq!(dialog.viewer_camera.fit_next, viewer_before.fit_next);

        frame(&mut dialog, &ctx);
        let viewer_before = dialog.viewer_camera;
        let root_before_viewer_click = dialog.camera;
        let viewer_minimap = dialog.test_action_rect("visual_viewer_minimap").unwrap().0;
        // Fit can make the viewport box cover the former 8px point. The
        // frame's outer margin remains outside that box and recenters.
        let viewer_click = viewer_minimap.left_top() + egui::vec2(2.0, 2.0);
        let projection = project_visual_scene(dialog.documents.visual_scene().unwrap()).unwrap();
        let canvas = dialog.test_viewer_canvas_rect.unwrap();
        let layout = minimap_layout_for_camera(&projection, canvas, canvas, viewer_minimap, dialog.viewer_camera).unwrap();
        assert!(!layout.viewport.contains_point(scene_point_from_screen(viewer_click)));
        for pressed in [true, false] {
            frame_with_viewer_events(
                &mut dialog,
                &ctx,
                vec![
                    egui::Event::PointerMoved(viewer_click),
                    egui::Event::PointerButton {
                        pos: viewer_click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_ne!(dialog.viewer_camera.center, viewer_before.center);
        assert_eq!(dialog.camera.center, root_before_viewer_click.center);
        assert_eq!(dialog.camera.zoom, root_before_viewer_click.zoom);
        assert_eq!(dialog.camera.fit_next, root_before_viewer_click.fit_next);
        assert_eq!(dialog.documents.visual_revision(), Some(revision));
    }

    #[test]
    fn diagram_studio_visual_canvas_created_object_becomes_only_primary_selection() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        click_action(&mut dialog, &ctx, "visual_add_shape");

        let created = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .map(|object| object.id)
            .find(|id| *id != crate::diagram::TITLE_OBJECT_ID)
            .unwrap();
        assert_eq!(dialog.visual_selection, Some(created));
        assert_eq!(
            dialog.visual_selected_objects,
            [created].into_iter().collect()
        );
    }

    #[test]
    fn diagram_studio_visual_canvas_viewer_navigation_keeps_editor_camera_independent() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        open_viewer(&mut dialog, &ctx);
        let editor_center = dialog.camera.center;
        let editor_zoom = dialog.camera.zoom;
        let viewer_zoom = dialog.viewer_camera.zoom;
        let revision = dialog.documents.visual_revision().unwrap();
        click_viewer_action(&mut dialog, &ctx, "visual_viewer_zoom_in");

        assert!(dialog.viewer_camera.zoom > viewer_zoom);
        assert_eq!(dialog.camera.center, editor_center);
        assert_eq!(dialog.camera.zoom, editor_zoom);
        assert_eq!(dialog.documents.visual_revision(), Some(revision));
    }

    fn frame_with_retired_owner_generation(dialog: &mut DiagramStudioDialog, ctx: &egui::Context) {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    TEST_VIEWPORT_SIZE,
                )),
                focused: true,
                ..Default::default()
            };
            let output = ctx.run_ui(raw_input, |ui| {
                let first_pass = ui.ctx().current_pass_index() == 0;
                dialog.show(ui.ctx(), first_pass, true, true);
            });
            output.drop_without_applying_deltas();
            dialog.owner_generation = dialog.owner_generation.wrapping_add(1);
            dialog.on_ui_frame_complete(ctx);
        });
    }

    fn wait_for_generic_preview(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
    ) -> std::sync::Arc<DiagramRenderOutput> {
        for _ in 0..300 {
            frame(dialog, ctx);
            let key = dialog
                .current_generic_key()
                .expect("the test document uses a source-rendered family");
            match &dialog.generic_preview {
                GenericPreviewStatus::Ready {
                    key: ready_key,
                    output,
                    ..
                } if ready_key == &key && dialog.test_generic_preview_drawn() => {
                    return std::sync::Arc::clone(output);
                }
                GenericPreviewStatus::Failed {
                    key: failed_key,
                    error,
                    ..
                } if failed_key == &key => {
                    panic!("generic fixture failed to render: {error}");
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
        panic!("generic preview did not complete within the bounded test wait");
    }

    fn svg_root_dimensions(svg: &str) -> (u32, u32) {
        let root = svg
            .split_once("<svg")
            .and_then(|(_, rest)| rest.split_once('>'))
            .map(|(root, _)| root)
            .expect("encoded SVG has a root element");
        let attribute = |name: &str| {
            let marker = format!("{name}=\"");
            root.split_once(&marker)
                .and_then(|(_, value)| value.split_once('"'))
                .and_then(|(value, _)| value.parse::<u32>().ok())
                .expect("encoded SVG root has integer natural dimensions")
        };
        (attribute("width"), attribute("height"))
    }

    fn generic_dialog(source: &str) -> DiagramStudioDialog {
        let mut dialog = DiagramStudioDialog::default();
        dialog.test_replace_source(source);
        dialog.open();
        dialog
    }

    #[test]
    fn diagram_studio_submits_generic_preview_only_after_retained_source_acceptance() {
        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        let ctx = egui::Context::default();
        let source = "mindmap\n  root((Accepted source))\n    qualified branch";
        let initial_revision = dialog.test_session().revision();

        dialog.test_pending_mutation = Some(PendingMutation::ReplaceSource(source.to_owned()));
        dialog.test_request_child_discard();
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_session().revision(), initial_revision);
        assert!(matches!(
            &dialog.generic_preview,
            GenericPreviewStatus::Idle
        ));
        assert!(dialog.renderer.is_none());

        for _ in 0..3 {
            if dialog.test_session().revision() != initial_revision {
                break;
            }
            dialog.test_pending_mutation = Some(PendingMutation::ReplaceSource(source.to_owned()));
            frame(&mut dialog, &ctx);
        }
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), initial_revision + 1);
        let current_key = match &dialog.generic_preview {
            GenericPreviewStatus::Pending { key, .. }
            | GenericPreviewStatus::Ready { key, .. }
            | GenericPreviewStatus::Failed { key, .. } => key,
            GenericPreviewStatus::Idle => panic!("accepted source was not submitted to renderer"),
        };
        assert_eq!(current_key.accepted_revision, initial_revision + 1);
        assert_eq!(current_key.document, dialog.render_document);
        assert!(dialog.renderer.is_some());

        dialog.accept_close();
        assert!(dialog.renderer.is_some());
        dialog.open();
        assert!(dialog.renderer.is_some());
    }

    #[test]
    fn studio_usability_diagram_generic_preview_is_current_and_stale_images_stay_editable() {
        let source = "mindmap\r\n  root((Launch))\r\n    Preview branch\r\n";
        let mut dialog = generic_dialog(source);
        let ctx = egui::Context::default();
        let output = wait_for_generic_preview(&mut dialog, &ctx);

        assert_eq!(output.family, crate::diagram::DiagramFamily::Mindmap);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().status().preview, PreviewState::Stale);
        assert!(dialog.test_session().preview_graph().is_some());
        assert!(dialog.test_session().current_graph().is_none());
        assert!(dialog.test_viewer_generic_preview_drawn());
        assert!(!dialog.test_native_inspector_visible());
        assert!(dialog.test_source_editor_enabled());
        assert_eq!(dialog.test_preview_status_text(), None);

        click_action(&mut dialog, &ctx, "menu_export");
        assert_eq!(dialog.test_action_enabled("export_svg"), Some(true));
        assert_eq!(dialog.test_action_enabled("export_png"), Some(true));
        click_action(&mut dialog, &ctx, "menu_export");
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_action_enabled("add_node"), Some(false));
        assert_eq!(dialog.test_action_enabled("auto_layout"), Some(false));
        assert_eq!(
            dialog
                .generic_texture
                .as_ref()
                .map(|texture| texture.key.accepted_revision),
            Some(dialog.test_session().revision())
        );

        let invalid_source = "gantt\r\n    title Stale plan\r\n    this statement is outside the supported source subset\r\n";
        dialog.test_replace_source(invalid_source);
        let invalid_revision = dialog.test_session().revision();
        let mut saw_stale_failure = false;
        for _ in 0..300 {
            frame(&mut dialog, &ctx);
            if matches!(
                &dialog.generic_preview,
                GenericPreviewStatus::Failed { key, .. }
                    if key.accepted_revision == invalid_revision
            ) {
                saw_stale_failure = dialog
                    .test_preview_status_text()
                    .is_some_and(|text| text.contains("STALE PREVIEW"));
                if saw_stale_failure {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            saw_stale_failure,
            "failed generic render must expose stale image state"
        );
        assert_eq!(dialog.test_session().draft(), invalid_source);
        assert!(dialog.test_generic_preview_drawn());
        assert!(dialog.test_source_editor_enabled());
        assert!(!dialog.test_native_inspector_visible());
        assert_eq!(dialog.test_action_enabled("add_node"), Some(false));
        assert_eq!(dialog.test_action_enabled("auto_layout"), Some(false));
    }

    #[test]
    fn studio_usability_diagram_generic_first_render_failure_has_no_false_pending_message() {
        let source = "pie\n    \"Unreadable value\" : not-a-number\n";
        let mut dialog = DiagramStudioDialog::default();
        dialog
            .documents
            .request_source_copy(DiagramSourceCopy::new(source).unwrap())
            .unwrap();
        dialog.open();
        let ctx = egui::Context::default();
        let revision = dialog.test_session().revision();

        let mut failed_without_image = false;
        for _ in 0..300 {
            frame(&mut dialog, &ctx);
            if matches!(
                &dialog.generic_preview,
                GenericPreviewStatus::Failed { key, .. } if key.accepted_revision == revision
            ) && dialog.test_generic_canvas_placeholder()
                == Some("No preview image is available.")
            {
                failed_without_image = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(failed_without_image);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(
            dialog.test_session().status().preview,
            PreviewState::NoPreview
        );
        assert!(dialog.test_session().current_graph().is_none());
        assert!(dialog.generic_texture.is_none());
        assert!(!dialog.test_generic_preview_drawn());
        assert!(dialog.test_source_editor_enabled());
        assert!(
            dialog
                .test_preview_status_text()
                .is_some_and(|text| text.starts_with("Mermaid preview unavailable:"))
        );
    }

    #[test]
    fn studio_usability_diagram_generic_exports_use_current_image_and_freeze_before_picker() {
        let directory = tempfile::tempdir().expect("temporary generic export directory");
        let source = "pie showData\r\n    title FrozenOriginal\r\n    \"Tea 🍵\" : 12\r\n    \"Bread\" : 8\r\n";
        let replacement = "pie showData\r\n    title FrozenReplacement\r\n    \"Rice\" : 20\r\n";
        let mut dialog = generic_dialog(source);
        let ctx = egui::Context::default();
        let output = wait_for_generic_preview(&mut dialog, &ctx);
        let revision = dialog.test_session().revision();
        let expected_dimensions = (
            output.natural_width.ceil() as u32,
            output.natural_height.ceil() as u32,
        );

        let png_path = directory.path().join("current.png");
        dialog.test_set_next_file_picker_result(Some(png_path.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Png,
            ExportScale::OneAndHalf,
        ));
        frame(&mut dialog, &ctx);
        let png = std::fs::read(&png_path).expect("generic PNG export was published");
        let decoded = image::load_from_memory(&png)
            .expect("generic PNG export decodes")
            .to_rgba8();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (
                (f64::from(output.natural_width) * 1.5).ceil() as u32,
                (f64::from(output.natural_height) * 1.5).ceil() as u32,
            )
        );
        assert!(
            decoded.pixels().filter(|pixel| pixel.0[3] > 0).count() > 500,
            "generic PNG export should contain rendered pixels"
        );
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Info(message)) if message.contains(&format!("revision {revision}"))
        ));

        let mermaid_path = directory.path().join("exact-source.mmd");
        dialog.test_set_next_file_picker_result(Some(mermaid_path.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Mermaid,
            ExportScale::One,
        ));
        frame(&mut dialog, &ctx);
        assert_eq!(std::fs::read(&mermaid_path).unwrap(), source.as_bytes());

        let occupied_path = directory.path().join("occupied.svg");
        let occupied_bytes = b"preserve existing generic export target";
        std::fs::write(&occupied_path, occupied_bytes).expect("prepare occupied SVG destination");
        dialog.test_set_next_file_picker_result(Some(occupied_path.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        frame(&mut dialog, &ctx);
        assert_eq!(std::fs::read(&occupied_path).unwrap(), occupied_bytes);
        assert!(matches!(&dialog.message, Some(StudioNotice::Error(_))));

        let svg_path = directory.path().join("frozen.svg");
        dialog.test_set_next_file_picker_result(Some(svg_path.clone()));
        dialog.test_set_picker_source_change(replacement);
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        frame(&mut dialog, &ctx);
        let svg = std::fs::read_to_string(&svg_path).expect("frozen generic SVG was published");
        assert!(svg.contains("FrozenOriginal"));
        assert!(!svg.contains("FrozenReplacement"));
        assert_eq!(svg_root_dimensions(&svg), expected_dimensions);
        assert_eq!(dialog.test_session().draft(), replacement);
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Info(message)) if message.contains(&format!("revision {revision}"))
        ));
        assert_eq!(dialog.test_file_picker_calls(), 4);
    }

    #[test]
    fn studio_usability_diagram_generic_same_frame_edit_waits_for_matching_render_before_export() {
        let directory = tempfile::tempdir().expect("temporary generic export directory");
        let destination = directory.path().join("too-early.svg");
        let source = "timeline\n    title Original\n    2025 : Planning\n";
        let updated = "timeline\n    title AcceptedEdit\n    2026 : Delivery\n";
        let mut dialog = generic_dialog(source);
        let ctx = egui::Context::default();
        let _ = wait_for_generic_preview(&mut dialog, &ctx);
        let original_revision = dialog.test_session().revision();

        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_pending_mutation = Some(PendingMutation::ReplaceSource(updated.to_owned()));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        frame(&mut dialog, &ctx);

        assert_eq!(dialog.test_session().draft(), updated);
        assert_eq!(dialog.test_session().revision(), original_revision + 1);
        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert!(!destination.exists());
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Error(message)) if message.contains("render is not ready")
        ));
        let output = wait_for_generic_preview(&mut dialog, &ctx);
        assert_eq!(output.family, crate::diagram::DiagramFamily::Timeline);
        assert!(output.svg.contains("AcceptedEdit"));
    }

    #[test]
    fn studio_usability_diagram_generic_discarded_export_pass_does_not_open_picker() {
        let directory = tempfile::tempdir().expect("temporary generic export directory");
        let destination = directory.path().join("discarded.svg");
        let source = "stateDiagram-v2\n    [*] --> Draft\n    Draft --> Review : submit\n";
        let mut dialog = generic_dialog(source);
        let ctx = egui::Context::default();
        let _ = wait_for_generic_preview(&mut dialog, &ctx);
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        frame_with_root_discard(&mut dialog, &ctx);
        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert!(!destination.exists());
        assert_eq!(dialog.test_session().draft(), source);
    }

    fn current_node_count(dialog: &DiagramStudioDialog) -> usize {
        dialog
            .test_session()
            .current_graph()
            .expect("test source remains valid")
            .nodes
            .len()
    }

    fn current_node_positions(dialog: &DiagramStudioDialog) -> Vec<NodePosition> {
        dialog
            .test_session()
            .current_graph()
            .expect("test source remains valid")
            .nodes
            .iter()
            .map(|node| NodePosition {
                id: node.id.clone(),
                position: node.position,
            })
            .collect()
    }

    fn action_center(dialog: &DiagramStudioDialog, name: &str) -> egui::Pos2 {
        dialog
            .test_action_rects
            .iter()
            .find(|(action, _, _)| action == name)
            .map(|(_, rect, clip)| {
                assert!(
                    clip.contains_rect(*rect),
                    "control {name} is clipped: rect={rect:?}, clip={clip:?}, pending={:?}",
                    dialog.documents.pending_transition()
                );
                rect.center()
            })
            .unwrap_or_else(|| {
                panic!(
                    "control {name} missing; menu_scope={:?}, pending={:?}, actions={:?}",
                    dialog.active_menu_scope,
                    dialog.documents.pending_transition(),
                    dialog
                        .test_action_rects
                        .iter()
                        .map(|(action, _, _)| action)
                        .collect::<Vec<_>>()
                )
            })
    }

    fn reveal_visual_action(dialog: &mut DiagramStudioDialog, ctx: &egui::Context, name: &str) {
        for _ in 0..12 {
            frame(dialog, ctx);
            let (_, rect, clip) = dialog
                .test_action_rects
                .iter()
                .find(|(action, _, _)| action == name)
                .unwrap_or_else(|| panic!("missing {name} while scrolling"));
            if clip.contains_rect(*rect) {
                return;
            }
            let direction = if rect.bottom() > clip.bottom() {
                -120.0
            } else {
                120.0
            };
            let scroll = dialog
                .test_visual_scroll_rect
                .expect("visual owner scroll rect");
            let pos = egui::pos2(
                scroll.right() - 4.0,
                if direction < 0.0 {
                    scroll.bottom() - 4.0
                } else {
                    scroll.top() + 4.0
                },
            );
            assert!(
                scroll.contains(pos),
                "wheel point must be inside the owner scroll area"
            );
            assert!(
                !dialog
                    .test_visual_canvas_rect
                    .is_some_and(|canvas| canvas.contains(pos)),
                "wheel point must not target canvas zoom"
            );
            frame_with_child_events(
                dialog,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, direction),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let observation = dialog
            .test_action_rects
            .iter()
            .find(|(action, _, _)| action == name);
        panic!("control {name} remains clipped after owner scrolling: {observation:?}");
    }

    fn click_action(dialog: &mut DiagramStudioDialog, ctx: &egui::Context, name: &str) {
        frame(dialog, ctx);
        let click = action_center(dialog, name);
        for pressed in [true, false] {
            frame_with_child_events(
                dialog,
                ctx,
                vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    fn click_viewer_action(dialog: &mut DiagramStudioDialog, ctx: &egui::Context, name: &str) {
        frame(dialog, ctx);
        let click = action_center(dialog, name);
        for pressed in [true, false] {
            frame_with_viewer_events(
                dialog,
                ctx,
                vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    fn visual_canvas_world_point(dialog: &DiagramStudioDialog, world: ScenePoint) -> egui::Pos2 {
        let canvas = dialog
            .test_visual_canvas_rect
            .expect("visual canvas was laid out in the current frame")
            .shrink(12.0);
        world_to_screen(world, canvas, dialog.camera)
    }

    fn drag_visual_canvas(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        start: egui::Pos2,
        end: egui::Pos2,
    ) {
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame_with_child_events(
            dialog,
            ctx,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        frame_with_child_events(dialog, ctx, vec![egui::Event::PointerMoved(end)]);
        frame_with_child_events(
            dialog,
            ctx,
            vec![egui::Event::PointerMoved(end), button(end, false)],
        );
        frame(dialog, ctx);
    }

    fn click_action_with_child_discard(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        name: &str,
    ) {
        frame(dialog, ctx);
        let click = action_center(dialog, name);
        for pressed in [true, false] {
            if !pressed {
                dialog.test_request_child_discard();
            }
            frame_with_child_events(
                dialog,
                ctx,
                vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    fn click_action_with_root_discard(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        name: &str,
    ) {
        frame(dialog, ctx);
        let click = action_center(dialog, name);
        for pressed in [true, false] {
            super::super::render::with_immediate_viewport_test_events(
                vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                || frame_with_root_discard(dialog, ctx),
            );
        }
    }

    fn drag_action_by(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        name: &str,
        delta: egui::Vec2,
    ) {
        frame(dialog, ctx);
        let start = action_center(dialog, name);
        let end = start + delta;
        let pointer_button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame_with_child_events(
            dialog,
            ctx,
            vec![
                egui::Event::PointerMoved(start),
                pointer_button(start, true),
            ],
        );
        frame_with_child_events(dialog, ctx, vec![egui::Event::PointerMoved(end)]);
        frame_with_child_events(dialog, ctx, vec![pointer_button(end, false)]);
        frame(dialog, ctx);
    }

    fn focus_action_with_keyboard(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        name: &str,
    ) {
        for _ in 0..40 {
            if dialog.test_action_has_focus(name) {
                return;
            }
            send_child_key(dialog, ctx, egui::Key::Tab, egui::Modifiers::NONE);
        }
        assert!(
            dialog.test_action_has_focus(name),
            "Tab should reach {name}"
        );
    }

    fn open_menu_with_keyboard(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
        trigger: &str,
        first_item: &str,
    ) {
        focus_action_with_keyboard(dialog, ctx, trigger);
        send_child_key(dialog, ctx, egui::Key::Space, egui::Modifiers::NONE);
        for _ in 0..4 {
            frame(dialog, ctx);
        }
        assert!(
            dialog.test_action_has_focus(first_item),
            "menu should focus {first_item}"
        );
    }

    #[test]
    fn diagram_studio_non_discarded_child_pass_commits_visual_transaction() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.test_session().revision();
        dialog.test_stage_add_node();
        frame(&mut dialog, &ctx);

        assert!(matches!(
            dialog.test_last_callback().1,
            Some(egui::ViewportClass::Immediate)
        ));
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert_eq!(current_node_count(&dialog), 2);
    }

    #[test]
    fn diagram_studio_child_discard_does_not_commit_visual_transaction() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.test_session().revision();
        dialog.test_stage_add_node();
        dialog.test_request_child_discard();
        frame(&mut dialog, &ctx);

        assert!(dialog.test_child_discard_requested);
        assert!(matches!(
            dialog.test_last_callback().1,
            Some(egui::ViewportClass::Immediate)
        ));
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(current_node_count(&dialog), 1);
    }

    #[test]
    fn diagram_studio_root_discard_after_show_drops_staged_visual_transaction() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.test_session().revision();
        dialog.test_stage_add_node();

        frame_with_root_discard(&mut dialog, &ctx);

        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(current_node_count(&dialog), 1);
    }

    #[test]
    fn diagram_studio_root_discard_does_not_publish_child_focus_intent() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();

        frame_with_root_discard(&mut dialog, &ctx);

        assert!(dialog.focus_source);
        assert!(dialog.needs_viewport_bootstrap());
        assert!(dialog.deferred_source_focus.is_none());
        frame(&mut dialog, &ctx);
        assert!(!dialog.needs_viewport_bootstrap());
        assert!(dialog.deferred_source_focus.is_some());
        frame(&mut dialog, &ctx);
        assert!(dialog.test_source_editor_has_focus(&ctx));
    }

    #[test]
    fn diagram_studio_source_tab_navigation_preserves_draft_and_allows_newlines() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        assert!(matches!(
            dialog.test_last_callback().1,
            Some(egui::ViewportClass::Immediate)
        ));
        assert!(dialog.test_source_editor_has_focus(&ctx));

        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();
        let undo_depth = dialog.test_session().undo_depth();
        let editor_id = dialog
            .test_source_editor_id
            .expect("source editor was rendered");
        let key_event = |key, modifiers, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };

        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::Tab, egui::Modifiers::NONE, true)],
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::Tab, egui::Modifiers::NONE, false)],
        );
        assert!(!dialog.test_source_editor_has_focus(&ctx));
        let following_widget = crate::gui::render::immediate_viewport_test_focused_id()
            .and_then(|(viewport, focused)| {
                (viewport == viewport_id()).then_some(focused).flatten()
            })
            .expect("Tab moves focus to a following live control in the child");
        assert_ne!(following_widget, editor_id);

        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::Tab, egui::Modifiers::SHIFT, true)],
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::Tab, egui::Modifiers::SHIFT, false)],
        );
        assert!(dialog.test_source_editor_has_focus(&ctx));
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);

        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::A, egui::Modifiers::COMMAND, true)],
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::A, egui::Modifiers::COMMAND, false)],
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::Text("flowchart TD".into())],
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::Enter, egui::Modifiers::NONE, true)],
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::Enter, egui::Modifiers::NONE, false)],
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::Text("    A[\"Typed\"]".into())],
        );
        assert_eq!(
            dialog.test_session().draft(),
            "flowchart TD\n    A[\"Typed\"]"
        );
        assert_eq!(dialog.test_session().revision(), revision + 3);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth + 3);
    }

    #[test]
    fn diagram_studio_embedded_fallback_uses_parent_surface_without_claiming_child_identity() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(true);

        // The first embedded Window pass only measures its initial geometry.
        // Settle it with parent input disabled before testing the visible UI.
        for _ in 0..3 {
            embedded_frame_with_events(&mut dialog, &ctx, Vec::new(), false);
        }

        let (callback_id, callback_class, body_rendered, _) = dialog.test_last_callback();
        assert_eq!(callback_id, Some(egui::ViewportId::ROOT));
        assert!(matches!(
            callback_class,
            Some(egui::ViewportClass::EmbeddedWindow)
        ));
        assert!(body_rendered);
        assert!(dialog.test_child_focused);
        assert!(!dialog.needs_viewport_bootstrap());
        assert_eq!(current_node_count(&dialog), 1);

        let click = action_center(&dialog, "add_node");
        let pointer_events = |pressed| {
            vec![
                egui::Event::PointerMoved(click),
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let revision = dialog.test_session().revision();
        embedded_frame_with_events(&mut dialog, &ctx, pointer_events(true), false);
        embedded_frame_with_events(&mut dialog, &ctx, pointer_events(false), false);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(current_node_count(&dialog), 1);

        // Re-publish enabled hit records under the authorized, focused parent.
        embedded_frame_with_events(&mut dialog, &ctx, Vec::new(), true);
        assert!(dialog.test_child_focused);
        let click = action_center(&dialog, "add_node");
        let pointer_events = |pressed| {
            vec![
                egui::Event::PointerMoved(click),
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        embedded_frame_with_events(&mut dialog, &ctx, pointer_events(true), true);
        embedded_frame_with_events(&mut dialog, &ctx, pointer_events(false), true);
        let (callback_id, callback_class, body_rendered, _) = dialog.test_last_callback();
        assert_eq!(callback_id, Some(egui::ViewportId::ROOT));
        assert!(matches!(
            callback_class,
            Some(egui::ViewportClass::EmbeddedWindow)
        ));
        assert!(body_rendered);
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert_eq!(current_node_count(&dialog), 2);
    }

    #[test]
    fn diagram_studio_close_pending_waits_for_an_accepted_root_pass() {
        let mut dialog = initialized_dialog();
        dialog.request_close();
        let ctx = egui::Context::default();
        let raw_input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                TEST_VIEWPORT_SIZE,
            )),
            focused: true,
            ..Default::default()
        };

        let output = ctx.run_ui(raw_input(), |ui| dialog.show(ui.ctx(), false, true, true));
        output.drop_without_applying_deltas();
        assert!(!dialog.on_ui_frame_complete(&ctx));
        assert!(dialog.close_pending);

        let output = ctx.run_ui(raw_input(), |ui| {
            dialog.show(ui.ctx(), ui.ctx().current_pass_index() == 0, true, true);
            if ui.ctx().current_pass_index() == 0 {
                ui.ctx()
                    .request_discard("Diagram Studio close root discard test");
            }
        });
        output.drop_without_applying_deltas();
        assert!(!dialog.on_ui_frame_complete(&ctx));
        assert!(dialog.close_pending);
        assert!(dialog.documents.pending_transition().is_none());

        let output = ctx.run_ui(raw_input(), |ui| dialog.show(ui.ctx(), true, true, true));
        output.drop_without_applying_deltas();
        assert!(!dialog.on_ui_frame_complete(&ctx));
        assert!(!dialog.close_pending);
        assert!(dialog.open);
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
    }

    #[test]
    fn diagram_studio_same_frame_source_edit_survives_ctrl_w_close() {
        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        assert!(dialog.test_source_editor_has_focus(&ctx));
        assert!(!dialog.documents.is_dirty());
        let revision = dialog.test_session().revision();
        let edited_source = "flowchart TD\n    A[\"Typed before close\"]";
        let modifiers = egui::Modifiers::COMMAND;

        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Text(edited_source.into()),
                egui::Event::Key {
                    key: egui::Key::W,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
            ],
        );

        assert!(dialog.open, "dirty close must wait for a decision");
        assert_eq!(dialog.test_session().draft(), edited_source);
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert!(dialog.documents.is_dirty());
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        assert_eq!(dialog.test_file_picker_calls(), 0);
    }

    #[test]
    fn diagram_studio_rejected_source_edit_aborts_same_frame_save() {
        let directory = tempfile::tempdir().expect("temporary native document directory");
        let destination = directory.path().join("must-not-save");
        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        assert!(dialog.test_source_editor_has_focus(&ctx));
        let modifiers = egui::Modifiers::COMMAND;
        let oversized_text = "x".repeat(crate::diagram::MAX_MERMAID_SOURCE_BYTES + 1);

        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Text(oversized_text),
                egui::Event::Key {
                    key: egui::Key::S,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
            ],
        );

        assert!(dialog.open);
        assert_eq!(dialog.test_session().draft(), "");
        assert!(!dialog.documents.is_dirty());
        assert!(dialog.documents.pending_transition().is_none());
        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert!(!destination.exists());
        assert!(
            dialog
                .message
                .as_ref()
                .map(StudioNotice::text)
                .is_some_and(|message| {
                    message.contains("source exceeds") && message.contains("byte limit")
                })
        );
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Error(message)) if message.contains("source exceeds")
        ));
    }

    #[test]
    fn diagram_studio_native_intents_wait_for_retained_root_and_save_untitled() {
        let directory = tempfile::tempdir().expect("temporary native document directory");
        let target_path = directory.path().join("target.mld");
        let mut target = DiagramDocumentController::new();
        target
            .replace_source("flowchart TD\n    Target[\"Loaded\"]")
            .expect("target source is valid");
        target
            .save_as(&target_path)
            .expect("persist native target for the UI open action");

        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        let original_draft = dialog.test_session().draft().to_owned();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        dialog.test_set_next_file_picker_result(Some(target_path.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Open);
        frame_with_root_discard(&mut dialog, &ctx);
        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert_eq!(dialog.test_session().draft(), original_draft);
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert_eq!(dialog.test_session().draft(), original_draft);

        dialog.test_set_next_file_picker_result(Some(target_path.clone()));
        click_action(&mut dialog, &ctx, "document_open");
        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert_eq!(
            dialog.test_session().draft(),
            "flowchart TD\n    Target[\"Loaded\"]"
        );
        assert_eq!(dialog.documents.path(), Some(target_path.as_path()));
        assert!(dialog.test_session().can_undo());
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_file_picker_calls(), 1);

        let invalid_path = directory.path().join("invalid.mld");
        std::fs::write(&invalid_path, b"not a native diagram document")
            .expect("prepare malformed Open target");
        let current_source = dialog.test_session().draft().to_owned();
        dialog.test_set_next_file_picker_result(Some(invalid_path.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Open);
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_file_picker_calls(), 2);
        assert_eq!(dialog.test_session().draft(), current_source);
        assert_eq!(dialog.documents.path(), Some(target_path.as_path()));
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Error(message)) if message.contains("invalid.mld")
        ));

        let save_path = directory.path().join("untitled");
        let mut blank = DiagramStudioDialog::default();
        blank.open();
        blank.test_set_next_file_picker_result(Some(save_path.clone()));
        let save_ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut blank, &save_ctx);
        }
        click_action(&mut blank, &save_ctx, "document_save");
        let saved_path = directory.path().join("untitled.mld");
        assert!(saved_path.is_file());
        assert_eq!(blank.documents.path(), Some(saved_path.as_path()));
        assert!(!blank.documents.is_dirty());
        assert!(matches!(
            &blank.message,
            Some(StudioNotice::Info(message)) if message.starts_with("Saved ")
        ));

        let shortcut_path = directory.path().join("shortcut-save");
        let mut shortcut = DiagramStudioDialog::default();
        shortcut.open();
        shortcut.test_set_next_file_picker_result(Some(shortcut_path));
        let shortcut_ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut shortcut, &shortcut_ctx);
        }
        send_child_key(
            &mut shortcut,
            &shortcut_ctx,
            egui::Key::S,
            egui::Modifiers::COMMAND,
        );
        assert!(directory.path().join("shortcut-save.mld").is_file());
        assert_eq!(shortcut.test_file_picker_calls(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn diagram_studio_windows_alt_space_cannot_activate_focused_discard() {
        let directory = tempfile::tempdir().expect("temporary native document directory");
        let saved_path = directory.path().join("saved-before-close.mld");
        let mut dialog = initialized_dialog();
        dialog
            .documents
            .save_as(&saved_path)
            .expect("establish a saved baseline");
        let saved_bytes = std::fs::read(&saved_path).expect("read saved baseline");
        dialog.test_replace_source("flowchart TD\n    A[\"Dirty before close\"]");
        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();
        let undo_depth = dialog.test_session().undo_depth();
        let redo_depth = dialog.test_session().redo_depth();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        dialog.request_close();
        frame(&mut dialog, &ctx);
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        frame(&mut dialog, &ctx);
        let discard_id = dialog
            .test_discard_button_id
            .expect("pending Close renders its Discard button");
        dialog.test_move_child_focus_to(discard_id);
        frame(&mut dialog, &ctx);
        assert_eq!(
            super::super::render::immediate_viewport_test_focused_id(),
            Some((viewport_id(), Some(discard_id))),
            "the real rendered Discard control owns focus"
        );

        let alt = egui::Modifiers::ALT;
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(alt),
                egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: alt,
                },
                egui::Event::Text(" ".into()),
                egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: alt,
                },
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ],
        );
        assert!(dialog.open);
        assert!(dialog.test_has_active_document());
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert_eq!(dialog.test_session().redo_depth(), redo_depth);
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        assert_eq!(
            std::fs::read(&saved_path).expect("read unchanged saved baseline"),
            saved_bytes
        );

        send_child_key(&mut dialog, &ctx, egui::Key::Space, egui::Modifiers::NONE);
        assert!(
            !dialog.open,
            "ordinary Space still activates focused Discard"
        );
        assert!(!dialog.test_has_active_document());
        assert_eq!(
            std::fs::read(&saved_path).expect("Discard leaves saved baseline unchanged"),
            saved_bytes
        );
    }

    #[cfg(windows)]
    #[test]
    fn diagram_studio_windows_alt_space_filters_only_its_associated_source_text() {
        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        assert!(dialog.test_source_editor_has_focus(&ctx));
        let revision = dialog.test_session().revision();
        let alt = egui::Modifiers::ALT;
        let alt_gr = egui::Modifiers::ALT | egui::Modifiers::CTRL;
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(alt),
                egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: alt,
                },
                egui::Event::Text(" ".into()),
                egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: alt,
                },
                egui::Event::ModifiersChanged(alt_gr),
                egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: alt_gr,
                },
                egui::Event::Text(" ".into()),
                egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: alt_gr,
                },
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                egui::Event::Text("ordinary".into()),
                egui::Event::Text(" ".into()),
            ],
        );

        assert_eq!(dialog.test_session().draft(), " ordinary ");
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert!(dialog.open);
        assert!(dialog.documents.pending_transition().is_none());
    }

    #[test]
    fn diagram_studio_os_viewport_close_requests_cancel_until_dirty_decision() {
        let directory = tempfile::tempdir().expect("temporary native document directory");
        let saved_path = directory.path().join("os-close-baseline.mld");
        let mut dialog = initialized_dialog();
        dialog
            .documents
            .save_as(&saved_path)
            .expect("establish a saved baseline");
        let saved_bytes = std::fs::read(&saved_path).expect("read saved baseline");
        dialog.test_replace_source("flowchart TD\n    A[\"Dirty before OS close\"]");
        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();
        let undo_depth = dialog.test_session().undo_depth();
        let redo_depth = dialog.test_session().redo_depth();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        let close_pass_commands =
            super::super::render::with_immediate_viewport_test_close_request(viewport_id(), || {
                frame_with_viewport_commands(&mut dialog, &ctx)
            });
        assert!(dialog.open);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert_eq!(dialog.test_session().redo_depth(), redo_depth);
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        assert!(
            !close_pass_commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close))
        );

        let cancel_close_commands = frame_with_viewport_commands(&mut dialog, &ctx);
        assert!(
            cancel_close_commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
        );
        assert!(
            !cancel_close_commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close))
        );
        let replay_commands = frame_with_viewport_commands(&mut dialog, &ctx);
        assert!(!replay_commands.iter().any(|command| {
            matches!(
                command,
                egui::ViewportCommand::CancelClose | egui::ViewportCommand::Close
            )
        }));
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close),
            "the close event is injected once while the dirty decision remains pending"
        );

        click_action(&mut dialog, &ctx, "decision_cancel");
        assert!(dialog.open);
        assert!(dialog.documents.pending_transition().is_none());
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert_eq!(dialog.test_session().redo_depth(), redo_depth);
        assert_eq!(dialog.documents.path(), Some(saved_path.as_path()));
        assert_eq!(
            std::fs::read(&saved_path).expect("Cancel retains saved file bytes"),
            saved_bytes
        );
    }

    #[test]
    fn diagram_studio_open_same_revision_rebuilds_scene_and_clears_selection_and_properties() {
        let directory = tempfile::tempdir().expect("temporary native document directory");
        let path_a = directory.path().join("same-revision-a.mld");
        let path_b = directory.path().join("same-revision-b.mld");
        let source_a = "flowchart TD\n    A[\"Alpha\"]";
        let source_b = "flowchart LR\n    B[\"Beta\"]";
        for (path, source) in [(&path_a, source_a), (&path_b, source_b)] {
            let mut document = DiagramDocumentController::new();
            document
                .replace_source(source)
                .expect("fixture source parses");
            assert_eq!(document.session().expect("fixture session").revision(), 1);
            document.save_as(path).expect("save same-revision fixture");
        }

        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        dialog.test_set_next_file_picker_result(Some(path_a.clone()));
        click_action(&mut dialog, &ctx, "document_open");
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.documents.path(), Some(path_a.as_path()));
        assert_eq!(dialog.test_session().revision(), 1);
        let (node_a_bounds, canvas, camera) = {
            let scene = dialog
                .cache
                .as_ref()
                .and_then(|cache| cache.scene.as_ref())
                .expect("Open A builds a scene");
            assert_eq!(scene.nodes[0].id.as_str(), "A");
            (
                scene.nodes[0].bounds,
                dialog.test_canvas_rect.expect("canvas rendered"),
                dialog.camera,
            )
        };
        let click = world_rect_to_screen(node_a_bounds, canvas, camera).center();
        for pressed in [true, false] {
            frame_with_child_events(
                &mut dialog,
                &ctx,
                vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        frame(&mut dialog, &ctx);
        assert_eq!(
            dialog.selection,
            Some(DiagramElement::Node(NodeId::new("A").unwrap()))
        );
        assert!(dialog.property_draft.is_some());

        dialog.test_set_next_file_picker_result(Some(path_b.clone()));
        click_action(&mut dialog, &ctx, "document_open");
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.documents.path(), Some(path_b.as_path()));
        assert_eq!(dialog.test_session().revision(), 1);
        let cache = dialog
            .cache
            .as_ref()
            .expect("Open B retains revision cache");
        assert_eq!(cache.revision, 1);
        let scene = cache.scene.as_ref().expect("Open B rebuilds its scene");
        assert_eq!(scene.nodes.len(), 1);
        assert_eq!(scene.nodes[0].id.as_str(), "B");
        assert_eq!(
            dialog.test_session().current_graph().unwrap().nodes[0].label,
            "Beta"
        );
        assert!(dialog.selection.is_none());
        assert!(dialog.selected_nodes.is_empty());
        assert!(dialog.property_draft.is_none());
    }

    #[test]
    fn diagram_studio_constructor_injects_data_root_without_creating_files() {
        let directory = tempfile::tempdir().expect("temporary app data root");
        let root = crate::platform::app_data::AppDataRoot::from_path(directory.path());
        let dialog = DiagramStudioDialog::new(&root);

        assert_eq!(dialog.default_directory.as_path(), directory.path());
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("inspect temporary root")
                .count(),
            0,
            "constructing an untitled dialog performs no filesystem writes"
        );
    }

    #[test]
    fn diagram_studio_close_decision_orders_edits_and_explicit_reopen_starts_blank() {
        let directory = tempfile::tempdir().expect("temporary native document directory");
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        let before_revision = dialog.test_session().revision();
        dialog.test_stage_add_node();
        dialog.test_stage_document_intent(DocumentIntent::Close);
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_session().revision(), before_revision + 1);
        assert_eq!(current_node_count(&dialog), 2);
        assert!(dialog.open);
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );

        let edited_source = dialog.test_session().draft().to_owned();
        dialog.ensure_open();
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close),
            "passive pin maintenance preserves a close decision"
        );
        click_action(&mut dialog, &ctx, "decision_cancel");
        assert!(dialog.open);
        assert!(dialog.documents.pending_transition().is_none());
        assert_eq!(dialog.test_session().draft(), edited_source);
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Info(message)) if message == "Document action cancelled."
        ));

        send_child_key(&mut dialog, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        dialog.test_set_next_file_picker_result(None);
        click_action(&mut dialog, &ctx, "decision_save");
        assert!(
            dialog.open,
            "cancelling Save As must leave the close decision open"
        );
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        assert!(dialog.documents.is_dirty());

        let occupied_path = directory.path().join("occupied.mld");
        std::fs::write(&occupied_path, b"preserve this existing file")
            .expect("prepare occupied Save As destination");
        dialog.test_set_next_file_picker_result(Some(occupied_path.clone()));
        click_action(&mut dialog, &ctx, "decision_save");
        assert!(
            dialog.open,
            "a failed Save As must leave the close decision open"
        );
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        assert!(dialog.documents.is_dirty());
        assert_eq!(
            std::fs::read(&occupied_path).expect("read occupied Save As destination"),
            b"preserve this existing file"
        );

        let saved_path = directory.path().join("saved-on-close");
        dialog.test_set_next_file_picker_result(Some(saved_path.clone()));
        click_action(&mut dialog, &ctx, "decision_save");
        assert!(!dialog.open);
        assert!(dialog.documents.session().is_none());
        let saved_path = directory.path().join("saved-on-close.mld");
        assert!(saved_path.is_file());
        assert_eq!(dialog.test_file_picker_calls(), 3);
        let mut reopened = DiagramDocumentController::new();
        assert_eq!(
            reopened
                .request_open(&saved_path)
                .expect("reopen saved native document"),
            TransitionOutcome::Applied
        );
        assert_eq!(
            reopened.session().expect("reopened session").draft(),
            edited_source
        );
        assert!(reopened.session().expect("reopened session").can_undo());
        let saved_bytes = std::fs::read(&saved_path).expect("capture saved document bytes");

        dialog.ensure_open();
        assert!(
            !dialog.open,
            "passive ensure must not revive a closed document"
        );
        dialog.open();
        assert!(dialog.open);
        assert_eq!(dialog.test_session().draft(), "");
        assert_eq!(dialog.test_session().revision(), 0);
        assert!(!dialog.test_session().can_undo());

        dialog.test_replace_source("flowchart TD\n    Discard[\"Unsaved\"]");
        dialog.request_close();
        frame(&mut dialog, &ctx);
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        click_action(&mut dialog, &ctx, "decision_discard");
        assert!(!dialog.open);
        assert!(dialog.documents.session().is_none());
        assert_eq!(
            std::fs::read(&saved_path).expect("saved file is unchanged after discard"),
            saved_bytes
        );
        dialog.ensure_open();
        assert!(!dialog.open);
        dialog.open();
        assert_eq!(dialog.test_session().draft(), "");
    }

    #[test]
    fn diagram_studio_new_nodes_start_at_the_view_then_search_outward() {
        let graph = DiagramDocument::default();
        let center = ScenePoint {
            x: 240.0,
            y: -120.0,
        };
        let (_, first) = next_node(&graph, center).unwrap();
        assert_eq!(first, Position::new(240.0, -120.0).unwrap());

        let mut occupied = graph;
        occupied.nodes.push(crate::diagram::DiagramNode {
            id: NodeId::new("Existing").unwrap(),
            label: "Existing".into(),
            shape: NodeShape::Rectangle,
            position: first,
            group: None,
        });
        let (_, second) = next_node(&occupied, center).unwrap();
        assert_ne!(second, first);
        assert!(
            f64::from(second.x - first.x).hypot(f64::from(second.y - first.y))
                >= DRAG_NODE_SEPARATION
        );
    }

    #[test]
    fn diagram_studio_minimum_width_stacks_split_and_keeps_visual_controls_reachable() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let size = egui::vec2(480.0, 360.0);
        dialog.test_begin_frame();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                focused: true,
                ..Default::default()
            },
            |ui| {
                let mut pending = None;
                let mut document_intent = None;
                let mut canvas_focus = None;
                let mut source_focus = None;
                dialog.draw_body(
                    ui,
                    true,
                    true,
                    false,
                    &mut pending,
                    &mut document_intent,
                    &mut canvas_focus,
                    &mut source_focus,
                    true,
                );
            },
        );
        output.drop_without_applying_deltas();

        let canvas = dialog
            .test_canvas_rect
            .expect("Split draws its visual canvas");
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let action_rects = dialog
            .test_action_rects
            .iter()
            .map(|(name, rect, clip)| (name.as_str(), *rect, *clip))
            .collect::<Vec<_>>();
        assert!(
            canvas.width() >= 220.0,
            "canvas geometry is too narrow: canvas={canvas:?}, viewport={viewport:?}, actions={action_rects:#?}"
        );
        assert!(
            canvas.left() >= viewport.left() && canvas.right() <= viewport.right(),
            "canvas must stay inside viewport width: canvas={canvas:?}, viewport={viewport:?}, actions={action_rects:#?}"
        );
        for name in [
            "menu_import",
            "menu_templates",
            "menu_export",
            "auto_layout",
            "direction",
            "zoom_out",
            "zoom_percentage",
            "zoom_in",
            "fit",
            "zoom_100",
        ] {
            let (_, rect, clip) = dialog
                .test_action_rects
                .iter()
                .find(|(action, _, _)| action == name)
                .unwrap_or_else(|| panic!("the narrow toolbar exposes {name}"));
            assert!(clip.contains_rect(*rect), "{name} is clipped: {rect:?}");
            assert!(rect.left() >= 0.0 && rect.right() <= size.x);
        }
        let (_, add_node, clip) = dialog
            .test_action_rects
            .iter()
            .find(|(name, _, _)| name == "add_node")
            .expect("the visual toolbar exposes Add node");
        assert!(clip.left() <= add_node.left() && clip.right() >= add_node.right());
        assert!(add_node.left() >= 0.0 && add_node.right() <= size.x);
    }

    #[test]
    fn diagram_studio_escape_dismisses_import_menu_before_closing_document() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        click_action(&mut dialog, &ctx, "menu_import");
        assert!(
            dialog
                .test_action_rects
                .iter()
                .any(|(name, _, _)| { name == "import_mermaid" })
        );
        send_child_key(&mut dialog, &ctx, egui::Key::Escape, egui::Modifiers::NONE);

        assert!(dialog.open, "Escape should dismiss the open menu first");
        assert!(dialog.documents.pending_transition().is_none());
        assert!(
            !dialog
                .test_action_rects
                .iter()
                .any(|(name, _, _)| name == "import_mermaid")
        );
    }

    #[test]
    fn diagram_studio_keyboard_import_menu_activates_one_picker_without_early_source_edits() {
        let directory = tempfile::tempdir().expect("temporary Mermaid import directory");
        let import_path = directory.path().join("keyboard.mmd");
        let imported_source = "flowchart LR\nB[\"Keyboard import\"]\n";
        std::fs::write(&import_path, imported_source.as_bytes()).expect("write Mermaid source");

        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let original_source = dialog.test_session().draft().to_owned();
        let original_revision = dialog.test_session().revision();
        let original_undo = dialog.test_session().undo_depth();
        let original_redo = dialog.test_session().redo_depth();
        dialog.test_set_next_file_picker_result(Some(import_path));

        open_menu_with_keyboard(&mut dialog, &ctx, "menu_import", "import_mermaid");
        send_child_key(
            &mut dialog,
            &ctx,
            egui::Key::ArrowDown,
            egui::Modifiers::NONE,
        );
        assert!(dialog.test_action_has_focus("import_mermaid"));
        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert_eq!(dialog.test_session().draft(), original_source.as_str());
        assert_eq!(dialog.test_session().revision(), original_revision);
        assert_eq!(dialog.test_session().undo_depth(), original_undo);
        assert_eq!(dialog.test_session().redo_depth(), original_redo);

        send_child_key(&mut dialog, &ctx, egui::Key::Enter, egui::Modifiers::NONE);

        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert_eq!(dialog.test_session().draft(), imported_source);
        assert_eq!(dialog.documents.path(), None);
        assert_eq!(dialog.test_session().revision(), original_revision + 1);
        assert_eq!(dialog.test_session().undo_depth(), original_undo + 1);
        assert_eq!(dialog.test_session().redo_depth(), 0);
        assert!(dialog.open);
    }

    #[test]
    fn diagram_studio_keyboard_template_and_export_menu_actions_run_once() {
        let directory = tempfile::tempdir().expect("temporary Mermaid export directory");
        let export_path = directory.path().join("keyboard-export.mmd");
        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let initial_source = dialog.test_session().draft().to_owned();
        let initial_revision = dialog.test_session().revision();
        let initial_undo = dialog.test_session().undo_depth();
        let initial_redo = dialog.test_session().redo_depth();

        open_menu_with_keyboard(&mut dialog, &ctx, "menu_templates", "template_linear");
        send_child_key(
            &mut dialog,
            &ctx,
            egui::Key::ArrowDown,
            egui::Modifiers::NONE,
        );
        assert!(dialog.test_action_has_focus("template_decision"));
        assert_eq!(dialog.test_session().draft(), initial_source.as_str());
        assert_eq!(dialog.test_session().revision(), initial_revision);
        assert_eq!(dialog.test_session().undo_depth(), initial_undo);
        assert_eq!(dialog.test_session().redo_depth(), initial_redo);

        send_child_key(&mut dialog, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert_eq!(
            dialog.test_session().draft(),
            DiagramTemplate::Decision.source()
        );
        assert_eq!(dialog.test_session().revision(), initial_revision + 1);
        assert_eq!(dialog.test_session().undo_depth(), initial_undo + 1);

        let template_source = dialog.test_session().draft().to_owned();
        let template_revision = dialog.test_session().revision();
        let template_undo = dialog.test_session().undo_depth();
        let template_redo = dialog.test_session().redo_depth();
        dialog.test_set_next_file_picker_result(Some(export_path.clone()));
        open_menu_with_keyboard(&mut dialog, &ctx, "menu_export", "export_mermaid");
        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert_eq!(dialog.test_session().draft(), template_source.as_str());
        assert_eq!(dialog.test_session().revision(), template_revision);
        assert_eq!(dialog.test_session().undo_depth(), template_undo);
        assert_eq!(dialog.test_session().redo_depth(), template_redo);

        send_child_key(&mut dialog, &ctx, egui::Key::Enter, egui::Modifiers::NONE);

        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert_eq!(
            std::fs::read(export_path).unwrap(),
            template_source.as_bytes()
        );
        assert_eq!(dialog.test_session().draft(), template_source.as_str());
        assert_eq!(dialog.test_session().revision(), template_revision);
        assert_eq!(dialog.test_session().undo_depth(), template_undo);
        assert_eq!(dialog.test_session().redo_depth(), template_redo);
        assert_eq!(dialog.documents.path(), None);
        assert!(dialog.open);
    }

    #[test]
    fn diagram_studio_keyboard_menu_focus_is_retained_root_scoped_and_escape_restores_trigger() {
        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();
        let undo = dialog.test_session().undo_depth();
        let redo = dialog.test_session().redo_depth();

        open_menu_with_keyboard(&mut dialog, &ctx, "menu_export", "export_mermaid");
        assert!(dialog.deferred_menu_focus.is_none());

        frame_with_events_and_root_discard(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
                egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::SHIFT,
                },
                egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::SHIFT,
                },
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ],
        );
        assert!(
            dialog.deferred_menu_focus.is_none(),
            "a discarded ROOT pass must not publish menu focus"
        );
        assert_eq!(dialog.test_session().draft(), source.as_str());
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo);
        assert_eq!(dialog.test_session().redo_depth(), redo);

        send_child_key(&mut dialog, &ctx, egui::Key::Tab, egui::Modifiers::SHIFT);
        assert!(dialog.test_action_has_focus("export_scale_two"));
        send_child_key(&mut dialog, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert_eq!(dialog.export_scale, ExportScale::Two);
        assert!(dialog.open);
        assert_eq!(dialog.test_session().draft(), source.as_str());
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo);
        assert_eq!(dialog.test_session().redo_depth(), redo);

        open_menu_with_keyboard(&mut dialog, &ctx, "menu_export", "export_mermaid");
        send_child_key(&mut dialog, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(dialog.open, "Escape should close the menu, not the Studio");
        assert!(dialog.test_action_has_focus("menu_export"));
        assert_eq!(dialog.test_session().draft(), source.as_str());
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo);
        assert_eq!(dialog.test_session().redo_depth(), redo);
    }

    #[test]
    fn diagram_studio_default_split_keeps_canvas_visible_beside_inspector() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let size = egui::vec2(1180.0, 760.0);
        dialog.test_begin_frame();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                focused: true,
                ..Default::default()
            },
            |ui| {
                let mut pending = None;
                let mut document_intent = None;
                let mut canvas_focus = None;
                let mut source_focus = None;
                dialog.draw_body(
                    ui,
                    true,
                    true,
                    false,
                    &mut pending,
                    &mut document_intent,
                    &mut canvas_focus,
                    &mut source_focus,
                    true,
                );
            },
        );
        output.drop_without_applying_deltas();

        let canvas = dialog
            .test_canvas_rect
            .expect("default Split mode renders a canvas");
        let visible = canvas.intersect(egui::Rect::from_min_size(egui::Pos2::ZERO, size));
        assert!(canvas.width() >= 220.0, "canvas is too narrow: {canvas:?}");
        assert!(
            visible.height() >= 220.0,
            "the default window shows a useful canvas region: canvas={canvas:?}, visible={visible:?}"
        );
    }

    #[test]
    fn studio_usability_diagram_presentation_preview_visibility_restores_mode_and_state() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        dialog.camera = Camera {
            center: ScenePoint { x: 34.0, y: -17.0 },
            zoom: 1.35,
            fit_next: false,
        };
        dialog.set_selection(
            DiagramElement::Node(NodeId::new("A").expect("fixture node id is valid")),
            false,
        );
        frame(&mut dialog, &ctx);

        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();
        let dirty = dialog.documents.is_dirty();
        let undo_depth = dialog.test_session().undo_depth();
        let redo_depth = dialog.test_session().redo_depth();
        let selection = dialog.selection.clone();
        let selected_nodes = dialog.selected_nodes.clone();
        let camera = dialog.camera;
        let scene_builds = dialog.test_scene_build_count;

        click_action(&mut dialog, &ctx, "view_visual");
        assert_eq!(dialog.view_mode, ViewMode::Visual);
        click_action(&mut dialog, &ctx, "view_source");
        assert_eq!(
            dialog.view_mode,
            ViewMode::Source {
                restore: PreviewMode::Visual
            }
        );
        assert!(dialog.test_canvas_rect.is_none());
        assert!(
            dialog
                .test_action_rects
                .iter()
                .any(|(name, rect, clip)| { name == "show_preview" && clip.contains_rect(*rect) })
        );
        click_action(&mut dialog, &ctx, "show_preview");
        assert_eq!(dialog.view_mode, ViewMode::Visual);

        click_action(&mut dialog, &ctx, "view_split");
        assert_eq!(dialog.view_mode, ViewMode::Split);
        click_action(&mut dialog, &ctx, "view_source");
        click_action(&mut dialog, &ctx, "show_preview");
        assert_eq!(dialog.view_mode, ViewMode::Split);
        assert!(dialog.test_canvas_rect.is_some());
        assert_eq!(dialog.camera.center, camera.center);
        assert_eq!(dialog.camera.zoom, camera.zoom);
        assert_eq!(dialog.camera.fit_next, camera.fit_next);
        assert_eq!(dialog.selection, selection);
        assert_eq!(dialog.selected_nodes, selected_nodes);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.documents.is_dirty(), dirty);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert_eq!(dialog.test_session().redo_depth(), redo_depth);
        assert_eq!(dialog.test_scene_build_count, scene_builds);
    }

    #[test]
    fn studio_usability_diagram_presentation_zoom_controls_are_local_and_bounded() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        dialog.camera = Camera {
            center: ScenePoint { x: 23.0, y: 41.0 },
            zoom: 1.0,
            fit_next: false,
        };
        frame(&mut dialog, &ctx);
        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();
        let dirty = dialog.documents.is_dirty();
        let undo_depth = dialog.test_session().undo_depth();
        let redo_depth = dialog.test_session().redo_depth();
        let scene_builds = dialog.test_scene_build_count;

        click_action(&mut dialog, &ctx, "zoom_in");
        assert!((dialog.camera.zoom - CAMERA_KEYBOARD_ZOOM_FACTOR).abs() < 0.001);
        click_action(&mut dialog, &ctx, "zoom_out");
        assert!((dialog.camera.zoom - 1.0).abs() < 0.001);

        dialog.camera.zoom = 2.75;
        let center_before_100 = dialog.camera.center;
        click_action(&mut dialog, &ctx, "zoom_100");
        assert_eq!(dialog.camera.zoom, 1.0);
        assert_eq!(dialog.camera.center, center_before_100);
        assert!(!dialog.camera.fit_next);

        dialog.camera.zoom = 6.0;
        click_action(&mut dialog, &ctx, "fit");
        assert!((CAMERA_MIN_ZOOM..=CAMERA_MAX_ZOOM).contains(&dialog.camera.zoom));
        assert_ne!(dialog.camera.zoom, 6.0);
        assert!(!dialog.camera.fit_next);

        dialog.camera.zoom = CAMERA_MIN_ZOOM;
        click_action(&mut dialog, &ctx, "zoom_out");
        assert_eq!(dialog.camera.zoom, CAMERA_MIN_ZOOM);
        dialog.camera.zoom = CAMERA_MAX_ZOOM;
        click_action(&mut dialog, &ctx, "zoom_in");
        assert_eq!(dialog.camera.zoom, CAMERA_MAX_ZOOM);

        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.documents.is_dirty(), dirty);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert_eq!(dialog.test_session().redo_depth(), redo_depth);
        assert_eq!(dialog.test_scene_build_count, scene_builds);
    }

    #[test]
    fn studio_usability_diagram_presentation_splitters_resize_and_preserve_ratios() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        dialog.camera = Camera {
            center: ScenePoint { x: -12.0, y: 28.0 },
            zoom: 1.4,
            fit_next: false,
        };
        dialog.set_selection(
            DiagramElement::Node(NodeId::new("A").expect("fixture node id is valid")),
            false,
        );
        frame(&mut dialog, &ctx);

        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();
        let dirty = dialog.documents.is_dirty();
        let undo_depth = dialog.test_session().undo_depth();
        let redo_depth = dialog.test_session().redo_depth();
        let selection = dialog.selection.clone();
        let selected_nodes = dialog.selected_nodes.clone();
        let camera = dialog.camera;
        let scene_builds = dialog.test_scene_build_count;
        let initial_canvas_width = dialog
            .test_canvas_rect
            .expect("default split draws canvas")
            .width();

        drag_action_by(
            &mut dialog,
            &ctx,
            "source_preview_splitter",
            egui::vec2(-100.0, 0.0),
        );
        let outer_drag_canvas_width = dialog
            .test_canvas_rect
            .expect("source/preview split keeps canvas visible")
            .width();
        assert!(outer_drag_canvas_width > initial_canvas_width + 50.0);

        drag_action_by(
            &mut dialog,
            &ctx,
            "inspector_canvas_splitter",
            egui::vec2(-60.0, 0.0),
        );
        let inner_drag_canvas_width = dialog
            .test_canvas_rect
            .expect("inspector/canvas split keeps canvas visible")
            .width();
        assert!(
            inner_drag_canvas_width > outer_drag_canvas_width + 30.0,
            "inner drag did not widen canvas: initial={initial_canvas_width}, outer={outer_drag_canvas_width}, inner={inner_drag_canvas_width}, source_ratio={}, inspector_ratio={}",
            dialog.source_preview_split_ratio,
            dialog.inspector_canvas_split_ratio,
        );

        let source_preview_ratio = dialog.source_preview_split_ratio;
        let inspector_canvas_ratio = dialog.inspector_canvas_split_ratio;
        click_action(&mut dialog, &ctx, "view_source");
        assert_eq!(dialog.source_preview_split_ratio, source_preview_ratio);
        assert_eq!(dialog.inspector_canvas_split_ratio, inspector_canvas_ratio);
        click_action(&mut dialog, &ctx, "show_preview");
        assert_eq!(dialog.source_preview_split_ratio, source_preview_ratio);
        assert_eq!(dialog.inspector_canvas_split_ratio, inspector_canvas_ratio);

        frame_with_size(&mut dialog, &ctx, egui::vec2(760.0, 900.0));
        assert_eq!(dialog.source_preview_split_ratio, source_preview_ratio);
        assert_eq!(dialog.inspector_canvas_split_ratio, inspector_canvas_ratio);
        assert!(
            dialog
                .test_canvas_rect
                .expect("stacked layout keeps canvas visible")
                .width()
                >= CANVAS_MIN_WIDTH
        );
        frame_with_size(&mut dialog, &ctx, TEST_VIEWPORT_SIZE);
        assert_eq!(dialog.source_preview_split_ratio, source_preview_ratio);
        assert_eq!(dialog.inspector_canvas_split_ratio, inspector_canvas_ratio);

        assert_eq!(dialog.camera.center, camera.center);
        assert_eq!(dialog.camera.zoom, camera.zoom);
        assert_eq!(dialog.camera.fit_next, camera.fit_next);
        assert_eq!(dialog.selection, selection);
        assert_eq!(dialog.selected_nodes, selected_nodes);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.documents.is_dirty(), dirty);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert_eq!(dialog.test_session().redo_depth(), redo_depth);
        assert_eq!(dialog.test_scene_build_count, scene_builds);
    }

    #[test]
    fn studio_usability_diagram_presentation_split_ratios_reject_nonfinite_and_clamp_bounds() {
        let total_width = 1000.0;
        let divider = SPLITTER_WIDTH;
        let minimum_first = SOURCE_PANE_MIN_WIDTH;
        let minimum_second = PREVIEW_PANE_MIN_WIDTH;
        let lower = minimum_first / (total_width - divider);
        let upper = 1.0 - minimum_second / (total_width - divider);

        assert_eq!(
            validated_split_ratio(
                f32::NAN,
                total_width,
                divider,
                minimum_first,
                minimum_second,
                DEFAULT_SOURCE_PREVIEW_RATIO,
            ),
            DEFAULT_SOURCE_PREVIEW_RATIO
        );
        assert_eq!(
            validated_split_ratio(
                f32::INFINITY,
                total_width,
                divider,
                minimum_first,
                minimum_second,
                DEFAULT_SOURCE_PREVIEW_RATIO,
            ),
            DEFAULT_SOURCE_PREVIEW_RATIO
        );
        assert_eq!(
            validated_split_ratio(
                -10.0,
                total_width,
                divider,
                minimum_first,
                minimum_second,
                DEFAULT_SOURCE_PREVIEW_RATIO,
            ),
            lower
        );
        assert_eq!(
            validated_split_ratio(
                10.0,
                total_width,
                divider,
                minimum_first,
                minimum_second,
                DEFAULT_SOURCE_PREVIEW_RATIO,
            ),
            upper
        );
        assert_eq!(
            validated_split_ratio(
                0.2,
                500.0,
                divider,
                SOURCE_PANE_MIN_WIDTH,
                PREVIEW_PANE_MIN_WIDTH,
                DEFAULT_SOURCE_PREVIEW_RATIO,
            ),
            DEFAULT_SOURCE_PREVIEW_RATIO
        );
        assert!(
            split_pane_widths(
                f32::INFINITY,
                f32::NAN,
                minimum_first,
                minimum_second,
                DEFAULT_SOURCE_PREVIEW_RATIO,
            )
            .0
            .is_finite()
        );
    }

    #[test]
    fn diagram_studio_prepared_text_respects_canvas_and_parent_clips() {
        let parent = egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(100.0, 100.0));
        let canvas = egui::Rect::from_min_max(egui::pos2(50.0, 0.0), egui::pos2(120.0, 80.0));
        let line = egui::Rect::from_min_max(egui::pos2(0.0, 40.0), egui::pos2(90.0, 90.0));
        assert_eq!(
            prepared_text_line_clip(line, canvas, parent),
            egui::Rect::from_min_max(egui::pos2(50.0, 40.0), egui::pos2(90.0, 80.0))
        );
    }

    #[test]
    fn diagram_studio_group_moves_preserve_spacing_and_ten_unit_nudges() {
        let initial = [
            NodePosition {
                id: NodeId::new("A").unwrap(),
                position: Position::new(13.0, 21.0).unwrap(),
            },
            NodePosition {
                id: NodeId::new("B").unwrap(),
                position: Position::new(190.0, 21.0).unwrap(),
            },
        ];
        let moved = moved_positions(&initial, ScenePoint { x: 10.0, y: 0.0 }).unwrap();
        assert_eq!(moved[0].position, Position::new(23.0, 21.0).unwrap());
        assert_eq!(moved[1].position, Position::new(200.0, 21.0).unwrap());
        assert_eq!(
            moved[1].position.x - moved[0].position.x,
            initial[1].position.x - initial[0].position.x
        );
    }

    #[test]
    fn diagram_studio_canvas_selection_focus_enables_canvas_delete_shortcut() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let scene = dialog
            .cache
            .as_ref()
            .and_then(|cache| cache.scene.as_ref())
            .expect("valid source produces a canvas scene");
        let node = scene.nodes.first().expect("fixture contains a node");
        let canvas = dialog.test_canvas_rect.expect("canvas was rendered");
        let click = world_rect_to_screen(node.bounds, canvas, dialog.camera).center();

        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(click),
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(click),
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(matches!(dialog.selection, Some(DiagramElement::Node(_))));
        frame(&mut dialog, &ctx);
        assert!(dialog.test_canvas_has_focus());

        let revision = dialog.test_session().revision();
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert_eq!(current_node_count(&dialog), 0);
    }

    #[test]
    fn diagram_studio_canvas_keyboard_pan_zoom_and_arrow_nudges_are_scoped() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let (node_bounds, canvas, camera) = {
            let scene = dialog
                .cache
                .as_ref()
                .and_then(|cache| cache.scene.as_ref())
                .expect("valid source produces a canvas scene");
            (
                scene.nodes.first().expect("fixture contains a node").bounds,
                dialog.test_canvas_rect.expect("canvas was rendered"),
                dialog.camera,
            )
        };
        let click = world_rect_to_screen(node_bounds, canvas, camera).center();
        for pressed in [true, false] {
            frame_with_child_events(
                &mut dialog,
                &ctx,
                vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        frame(&mut dialog, &ctx);
        assert!(dialog.test_canvas_has_focus());
        assert_eq!(dialog.selected_nodes.len(), 1);

        let source = dialog.test_session().draft().to_owned();
        let positions_before_camera_keys = current_node_positions(&dialog);
        let revision_before_camera_keys = dialog.test_session().revision();
        let undo_before_camera_keys = dialog.test_session().undo_depth();
        let redo_before_camera_keys = dialog.test_session().redo_depth();
        let camera_before_keys = dialog.camera;

        send_child_key(
            &mut dialog,
            &ctx,
            egui::Key::ArrowRight,
            egui::Modifiers::COMMAND,
        );
        let expected_pan = CAMERA_KEYBOARD_PAN_SCREEN_POINTS / f64::from(camera_before_keys.zoom);
        assert_eq!(
            dialog.camera.center.x,
            camera_before_keys.center.x + expected_pan
        );
        assert_eq!(dialog.camera.center.y, camera_before_keys.center.y);
        assert_eq!(dialog.camera.zoom, camera_before_keys.zoom);

        send_child_key(
            &mut dialog,
            &ctx,
            egui::Key::Equals,
            egui::Modifiers::COMMAND,
        );
        let zoomed_in = dialog.camera.zoom;
        assert!(zoomed_in > camera_before_keys.zoom);
        send_child_key(
            &mut dialog,
            &ctx,
            egui::Key::Minus,
            egui::Modifiers::COMMAND,
        );
        assert!((dialog.camera.zoom - camera_before_keys.zoom).abs() < 1e-5);
        assert_eq!(
            dialog.camera.center,
            ScenePoint {
                x: camera_before_keys.center.x + expected_pan,
                y: camera_before_keys.center.y,
            }
        );
        assert_eq!(
            current_node_positions(&dialog),
            positions_before_camera_keys
        );
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(
            dialog.test_session().revision(),
            revision_before_camera_keys
        );
        assert_eq!(dialog.test_session().undo_depth(), undo_before_camera_keys);
        assert_eq!(dialog.test_session().redo_depth(), redo_before_camera_keys);

        send_child_key(
            &mut dialog,
            &ctx,
            egui::Key::ArrowRight,
            egui::Modifiers::NONE,
        );
        let after_small_nudge = current_node_positions(&dialog);
        assert_eq!(
            after_small_nudge[0].position.x,
            positions_before_camera_keys[0].position.x + 10.0
        );
        assert_eq!(
            dialog.test_session().revision(),
            revision_before_camera_keys + 1
        );
        assert_eq!(
            dialog.test_session().undo_depth(),
            undo_before_camera_keys + 1
        );

        send_child_key(
            &mut dialog,
            &ctx,
            egui::Key::ArrowRight,
            egui::Modifiers::SHIFT,
        );
        let after_large_nudge = current_node_positions(&dialog);
        assert_eq!(
            after_large_nudge[0].position.x,
            positions_before_camera_keys[0].position.x + 50.0
        );
        assert_eq!(
            dialog.test_session().revision(),
            revision_before_camera_keys + 2
        );
        assert_eq!(
            dialog.test_session().undo_depth(),
            undo_before_camera_keys + 2
        );
        assert_eq!(dialog.test_session().redo_depth(), redo_before_camera_keys);
        assert_eq!(
            dialog.camera.center,
            ScenePoint {
                x: camera_before_keys.center.x + expected_pan,
                y: camera_before_keys.center.y,
            }
        );
        assert!((dialog.camera.zoom - camera_before_keys.zoom).abs() < 1e-5);

        let before_collapsed_modifiers = current_node_positions(&dialog);
        let revision_before_collapsed_modifiers = dialog.test_session().revision();
        let undo_before_collapsed_modifiers = dialog.test_session().undo_depth();
        let key_event = |pressed| egui::Event::Key {
            key: egui::Key::ArrowRight,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        };
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
                key_event(true),
                key_event(false),
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ],
        );
        let after_collapsed_modifiers = current_node_positions(&dialog);
        assert_eq!(
            after_collapsed_modifiers[0].position.x,
            before_collapsed_modifiers[0].position.x + 40.0
        );
        assert_eq!(
            dialog.test_session().revision(),
            revision_before_collapsed_modifiers + 1
        );
        assert_eq!(
            dialog.test_session().undo_depth(),
            undo_before_collapsed_modifiers + 1
        );
        frame(&mut dialog, &ctx);
        assert_eq!(current_node_positions(&dialog), after_collapsed_modifiers);
        assert_eq!(
            dialog.test_session().revision(),
            revision_before_collapsed_modifiers + 1,
            "an idle frame must not repeat a consumed Shift-arrow edit"
        );
    }

    #[test]
    fn diagram_studio_mermaid_assistance_accepted_suggestion_is_one_undo_step_and_preserves_utf8_source()
     {
        let source = "flowchart TD\r\nA[\"東京\"]\r\n";
        let mut dialog = DiagramStudioDialog::default();
        dialog.test_replace_source(source);
        dialog.view_mode = ViewMode::Source {
            restore: PreviewMode::Split,
        };
        dialog.open();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        let editor_id = dialog
            .test_source_editor_id
            .expect("the Mermaid source editor was rendered");
        super::install_text_edit_cursor(&ctx, editor_id, source.chars().count());
        let revision = dialog.test_session().revision();
        let undo_depth = dialog.test_session().undo_depth();

        click_action(&mut dialog, &ctx, "mermaid_assist_menu");
        assert!(
            dialog
                .test_action_rects
                .iter()
                .any(|(action, _, _)| action == "mermaid_assist_suggestion_0")
        );
        click_action(&mut dialog, &ctx, "mermaid_assist_suggestion_0");

        let assisted = "flowchart TD\r\nA[\"東京\"]\r\nNode[\"New node\"]";
        assert_eq!(dialog.test_session().draft(), assisted);
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth + 1);
        assert_eq!(
            dialog.deferred_source_focus,
            Some((viewport_id(), editor_id))
        );

        // No idle frame is inserted between the accepted suggestion and the
        // next editor input. Focus must be restored before TextEdit processes it.
        frame_with_child_events(&mut dialog, &ctx, vec![egui::Event::Text("!".into())]);
        assert_eq!(dialog.test_session().draft(), format!("{assisted}!"));
        assert!(dialog.test_source_editor_has_focus(&ctx));
        assert!(dialog.deferred_source_focus.is_none());
    }

    #[test]
    fn diagram_studio_mermaid_assistance_discarded_child_pass_keeps_source_unchanged() {
        let source = "flowchart TD\r\nA[\"東京\"]\r\n";
        let mut dialog = DiagramStudioDialog::default();
        dialog.test_replace_source(source);
        dialog.view_mode = ViewMode::Source {
            restore: PreviewMode::Split,
        };
        dialog.open();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let editor_id = dialog
            .test_source_editor_id
            .expect("the Mermaid source editor was rendered");
        super::install_text_edit_cursor(&ctx, editor_id, source.chars().count());
        let revision = dialog.test_session().revision();
        let undo_depth = dialog.test_session().undo_depth();

        click_action(&mut dialog, &ctx, "mermaid_assist_menu");
        click_action_with_child_discard(&mut dialog, &ctx, "mermaid_assist_suggestion_0");

        assert!(dialog.test_child_discard_requested);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert!(dialog.deferred_source_focus.is_none());
    }

    #[test]
    fn diagram_studio_source_focus_does_not_pan_or_zoom_canvas() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        assert!(dialog.test_source_editor_has_focus(&ctx));
        let source = dialog.test_session().draft().to_owned();
        let positions = current_node_positions(&dialog);
        let revision = dialog.test_session().revision();
        let undo_depth = dialog.test_session().undo_depth();
        let camera = dialog.camera;

        send_child_key(
            &mut dialog,
            &ctx,
            egui::Key::ArrowRight,
            egui::Modifiers::COMMAND,
        );
        send_child_key(
            &mut dialog,
            &ctx,
            egui::Key::Equals,
            egui::Modifiers::COMMAND,
        );
        send_child_key(
            &mut dialog,
            &ctx,
            egui::Key::Minus,
            egui::Modifiers::COMMAND,
        );

        assert!(dialog.test_source_editor_has_focus(&ctx));
        assert_eq!(dialog.camera.center, camera.center);
        assert_eq!(dialog.camera.zoom, camera.zoom);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(current_node_positions(&dialog), positions);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
    }

    #[test]
    fn diagram_studio_pointer_drag_moves_selection_once_and_supports_undo_redo() {
        let mut dialog = DiagramStudioDialog::default();
        dialog.test_replace_source("flowchart LR\n    A[\"Alpha\"]\n    B[\"Beta\"]");
        dialog.open();
        let initial_positions = vec![
            NodePosition {
                id: NodeId::new("A").unwrap(),
                position: Position::new(13.0, 21.0).unwrap(),
            },
            NodePosition {
                id: NodeId::new("B").unwrap(),
                position: Position::new(190.0, 41.0).unwrap(),
            },
        ];
        dialog
            .documents
            .apply_transaction(&[GraphOperation::MoveNodes {
                positions: initial_positions.clone(),
            }])
            .unwrap();
        dialog.refresh_cache();

        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let (node_a_bounds, canvas, camera) = {
            let scene = dialog
                .cache
                .as_ref()
                .and_then(|cache| cache.scene.as_ref())
                .expect("valid source produces a canvas scene");
            let node_a = scene
                .nodes
                .iter()
                .find(|node| node.id.as_str() == "A")
                .expect("fixture contains node A");
            (
                node_a.bounds,
                dialog.test_canvas_rect.expect("canvas was rendered"),
                dialog.camera,
            )
        };
        let press = world_rect_to_screen(node_a_bounds, canvas, camera).center();
        let pointer_button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };

        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(press),
                pointer_button(press, true),
            ],
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(press),
                pointer_button(press, false),
            ],
        );
        frame(&mut dialog, &ctx);
        assert!(matches!(
            dialog.selection,
            Some(DiagramElement::Node(ref id)) if id.as_str() == "A"
        ));
        assert!(dialog.test_canvas_has_focus());

        let key_event = |key, modifiers, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::A, egui::Modifiers::COMMAND, true)],
        );
        assert_eq!(dialog.selected_nodes.len(), 2);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::A, egui::Modifiers::COMMAND, false)],
        );

        let positions_before_drag = current_node_positions(&dialog);
        assert_eq!(positions_before_drag, initial_positions);
        let revision_before_drag = dialog.test_session().revision();
        let undo_before_drag = dialog.test_session().undo_depth();
        let redo_before_drag = dialog.test_session().redo_depth();

        let drag_start = press + egui::vec2(9.0, 3.0);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(press),
                pointer_button(press, true),
            ],
        );
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(drag_start)],
        );
        assert!(dialog.drag.is_some(), "pointer movement starts a drag");
        let drag_origin = dialog.drag.as_ref().unwrap().pointer_origin;
        assert_eq!(dialog.drag.as_ref().unwrap().active_node.as_str(), "A");
        let drag_zoom = dialog.camera.zoom;
        assert_eq!(current_node_positions(&dialog), positions_before_drag);
        assert_eq!(dialog.test_session().revision(), revision_before_drag);
        assert_eq!(dialog.test_session().undo_depth(), undo_before_drag);
        assert_eq!(dialog.test_session().redo_depth(), redo_before_drag);
        frame(&mut dialog, &ctx);
        assert!(dialog.drag.is_some());
        assert_eq!(current_node_positions(&dialog), positions_before_drag);
        assert!(dialog.test_canvas_has_focus());

        let drag_end = drag_start + egui::vec2(27.0, 18.0);
        frame_with_child_events(&mut dialog, &ctx, vec![egui::Event::PointerMoved(drag_end)]);
        assert!(dialog.drag.is_some());
        assert_eq!(current_node_positions(&dialog), positions_before_drag);
        assert_eq!(dialog.test_session().revision(), revision_before_drag);
        assert_eq!(dialog.test_session().undo_depth(), undo_before_drag);
        assert_eq!(dialog.test_session().redo_depth(), redo_before_drag);

        let snapped_dx = ((f64::from(drag_end.x - drag_origin.x)
            / f64::from(drag_zoom)
            / f64::from(GRID_SPACING))
        .round()
            * f64::from(GRID_SPACING)) as f32;
        let snapped_dy = ((f64::from(drag_end.y - drag_origin.y)
            / f64::from(drag_zoom)
            / f64::from(GRID_SPACING))
        .round()
            * f64::from(GRID_SPACING)) as f32;
        assert_ne!((snapped_dx, snapped_dy), (0.0, 0.0));
        let positions_after_drag = positions_before_drag
            .iter()
            .map(|node| NodePosition {
                id: node.id.clone(),
                position: Position::new(node.position.x + snapped_dx, node.position.y + snapped_dy)
                    .unwrap(),
            })
            .collect::<Vec<_>>();

        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![
                egui::Event::PointerMoved(drag_end),
                pointer_button(drag_end, false),
            ],
        );
        assert!(dialog.drag.is_none());
        assert_eq!(current_node_positions(&dialog), positions_after_drag);
        assert_eq!(dialog.test_session().revision(), revision_before_drag + 1);
        assert_eq!(dialog.test_session().undo_depth(), undo_before_drag + 1);
        assert_eq!(dialog.test_session().redo_depth(), redo_before_drag);
        assert_eq!(
            positions_after_drag[1].position.x - positions_after_drag[0].position.x,
            positions_before_drag[1].position.x - positions_before_drag[0].position.x
        );
        assert_eq!(
            positions_after_drag[1].position.y - positions_after_drag[0].position.y,
            positions_before_drag[1].position.y - positions_before_drag[0].position.y
        );

        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_session().revision(), revision_before_drag + 1);
        assert_eq!(dialog.test_session().undo_depth(), undo_before_drag + 1);
        assert_eq!(dialog.test_session().redo_depth(), redo_before_drag);
        assert!(dialog.test_canvas_has_focus());

        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::Z, egui::Modifiers::COMMAND, true)],
        );
        assert_eq!(current_node_positions(&dialog), positions_before_drag);
        assert_eq!(dialog.test_session().revision(), revision_before_drag + 2);
        assert_eq!(dialog.test_session().undo_depth(), undo_before_drag);
        assert_eq!(dialog.test_session().redo_depth(), redo_before_drag + 1);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::Z, egui::Modifiers::COMMAND, false)],
        );
        assert_eq!(dialog.test_session().revision(), revision_before_drag + 2);

        let command_shift = egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::Z, command_shift, true)],
        );
        assert_eq!(current_node_positions(&dialog), positions_after_drag);
        assert_eq!(dialog.test_session().revision(), revision_before_drag + 3);
        assert_eq!(dialog.test_session().undo_depth(), undo_before_drag + 1);
        assert_eq!(dialog.test_session().redo_depth(), redo_before_drag);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![key_event(egui::Key::Z, command_shift, false)],
        );
        assert_eq!(dialog.test_session().revision(), revision_before_drag + 3);
    }

    #[test]
    fn diagram_studio_invalid_source_retains_last_good_preview_and_blocks_visual_edits() {
        let mut dialog = initialized_dialog();
        dialog.test_replace_source("flowchart TD\n    A -->");
        let revision = dialog.test_session().revision();
        let invalid_source = dialog.test_session().draft().to_owned();
        let last_good_nodes = dialog.test_session().preview_graph().unwrap().nodes.len();
        assert_eq!(dialog.test_session().status().preview, PreviewState::Stale);

        let id = NodeId::new("Blocked").unwrap();
        dialog.apply_pending(PendingMutation::Transaction {
            operations: vec![GraphOperation::AddNode {
                id,
                label: "Blocked".into(),
                shape: NodeShape::Rectangle,
                position: Position::new(900.0, 900.0).unwrap(),
                group: None,
            }],
            expected_revision: revision,
            select_after: None,
        });

        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().draft(), invalid_source);
        assert_eq!(
            dialog.test_session().preview_graph().unwrap().nodes.len(),
            last_good_nodes
        );
        assert!(
            dialog
                .message
                .as_ref()
                .map(StudioNotice::text)
                .is_some_and(|message| message.contains("invalid"))
        );
    }

    #[test]
    fn diagram_studio_stale_property_basis_cannot_overwrite_new_source() {
        let mut dialog = initialized_dialog();
        let old_revision = dialog.test_session().revision();
        dialog.test_replace_source("flowchart TD\n    A[\"Updated source\"]");
        let updated_revision = dialog.test_session().revision();

        dialog.apply_pending(PendingMutation::Transaction {
            operations: vec![GraphOperation::SetNodeLabel {
                id: NodeId::new("A").unwrap(),
                label: "Stale property draft".into(),
            }],
            expected_revision: old_revision,
            select_after: Some(DiagramElement::Node(NodeId::new("A").unwrap())),
        });

        let graph = dialog
            .test_session()
            .current_graph()
            .expect("updated source remains valid");
        assert_eq!(graph.nodes[0].label, "Updated source");
        assert_eq!(dialog.test_session().revision(), updated_revision);
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Error(message))
                if message.contains("changed while these properties were open")
        ));
    }

    #[test]
    fn diagram_studio_imports_unsupported_mermaid_and_exports_exact_draft() {
        let directory = tempfile::tempdir().expect("temporary Mermaid import directory");
        let import_path = directory.path().join("source.mmd");
        let source = "\u{feff}flowchart TD\r\nA[\"こんにちは\"]\r\nA --- B\r\n";
        std::fs::write(&import_path, source.as_bytes()).expect("write Mermaid import fixture");
        let export_path = directory.path().join("roundtrip.mmd");

        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        dialog.test_set_next_file_picker_result(Some(import_path));
        click_action(&mut dialog, &ctx, "menu_import");
        click_action(&mut dialog, &ctx, "import_mermaid");
        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.documents.path(), None);
        assert_eq!(
            dialog.test_session().status().preview,
            PreviewState::NoPreview
        );

        let revision = dialog.test_session().revision();
        let undo_depth = dialog.test_session().undo_depth();
        let redo_depth = dialog.test_session().redo_depth();
        let dirty = dialog.documents.is_dirty();
        let disk_digest = dialog.documents.disk_digest();
        dialog.test_set_next_file_picker_result(Some(export_path.clone()));
        click_action(&mut dialog, &ctx, "menu_export");
        click_action(&mut dialog, &ctx, "export_mermaid");

        assert_eq!(dialog.test_file_picker_calls(), 2);
        assert_eq!(std::fs::read(&export_path).unwrap(), source.as_bytes());
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert_eq!(dialog.test_session().redo_depth(), redo_depth);
        assert_eq!(dialog.documents.path(), None);
        assert_eq!(dialog.documents.disk_digest(), disk_digest);
        assert_eq!(dialog.documents.is_dirty(), dirty);
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Info(message))
                if message.contains("Mermaid source")
                    && message.contains(&format!("revision {revision}"))
                    && message.contains("roundtrip.mmd")
        ));
    }

    #[test]
    fn diagram_studio_same_frame_source_export_follows_edit_and_rejected_edit_aborts_picker() {
        let directory = tempfile::tempdir().expect("temporary export directory");
        let accepted_path = directory.path().join("accepted.mmd");
        let accepted_source = "flowchart LR\nA[\"Accepted source\"]\n";
        let mut accepted = DiagramStudioDialog::default();
        accepted.open();
        let accepted_ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut accepted, &accepted_ctx);
        }
        let revision = accepted.test_session().revision();
        accepted.test_set_next_file_picker_result(Some(accepted_path.clone()));
        accepted.test_pending_mutation =
            Some(PendingMutation::ReplaceSource(accepted_source.to_owned()));
        accepted.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Mermaid,
            ExportScale::One,
        ));
        frame(&mut accepted, &accepted_ctx);
        assert_eq!(accepted.test_file_picker_calls(), 1);
        assert_eq!(
            std::fs::read(&accepted_path).unwrap(),
            accepted_source.as_bytes()
        );
        assert_eq!(accepted.test_session().draft(), accepted_source);
        assert_eq!(accepted.test_session().revision(), revision + 1);
        assert_eq!(accepted.documents.path(), None);
        assert!(accepted.documents.is_dirty());

        let rejected_path = directory.path().join("rejected.mmd");
        let mut rejected = DiagramStudioDialog::default();
        rejected.open();
        let rejected_ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut rejected, &rejected_ctx);
        }
        let original_revision = rejected.test_session().revision();
        rejected.test_set_next_file_picker_result(Some(rejected_path.clone()));
        rejected.test_pending_mutation = Some(PendingMutation::ReplaceSource(
            "x".repeat(crate::diagram::MAX_MERMAID_SOURCE_BYTES + 1),
        ));
        rejected.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Mermaid,
            ExportScale::One,
        ));
        frame(&mut rejected, &rejected_ctx);
        assert_eq!(rejected.test_file_picker_calls(), 0);
        assert!(!rejected_path.exists());
        assert_eq!(rejected.test_session().draft(), "");
        assert_eq!(rejected.test_session().revision(), original_revision);
        assert!(!rejected.documents.is_dirty());
        assert!(matches!(
            &rejected.message,
            Some(StudioNotice::Error(message))
                if message.contains("source exceeds") && message.contains("byte limit")
        ));
    }

    #[test]
    fn diagram_studio_discarded_export_pass_does_not_open_picker_or_write() {
        let directory = tempfile::tempdir().expect("temporary export directory");
        let destination = directory.path().join("discarded.svg");
        let mut dialog = initialized_dialog();
        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();
        let undo_depth = dialog.test_session().undo_depth();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        frame_with_root_discard(&mut dialog, &ctx);

        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert!(!destination.exists());
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
    }

    #[test]
    fn diagram_studio_exports_do_not_change_document_state_on_success_cancel_or_failure() {
        let directory = tempfile::tempdir().expect("temporary export directory");
        let png_path = directory.path().join("diagram.png");
        let occupied_path = directory.path().join("occupied.mmd");
        let occupied_bytes = b"leave existing export untouched";
        std::fs::write(&occupied_path, occupied_bytes).expect("prepare occupied export target");

        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let original_source = dialog.test_session().draft().to_owned();
        let original_revision = dialog.test_session().revision();
        let original_undo = dialog.test_session().undo_depth();
        let original_redo = dialog.test_session().redo_depth();
        let original_dirty = dialog.documents.is_dirty();
        let original_path = dialog.documents.path().map(Path::to_path_buf);
        let original_digest = dialog.documents.disk_digest();

        dialog.test_set_next_file_picker_result(Some(png_path.clone()));
        click_action(&mut dialog, &ctx, "menu_export");
        click_action(&mut dialog, &ctx, "export_png");
        let png = std::fs::read(&png_path).expect("read published PNG");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(image::load_from_memory(&png).is_ok());
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Info(message))
                if message.contains(&format!("revision {original_revision}"))
                    && message.contains("diagram.png")
                    && message.contains("×")
        ));

        dialog.test_set_next_file_picker_result(None);
        click_action(&mut dialog, &ctx, "menu_export");
        click_action(&mut dialog, &ctx, "export_svg");
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Info(message)) if message.contains("cancelled")
        ));

        dialog.test_set_next_file_picker_result(Some(occupied_path.clone()));
        click_action(&mut dialog, &ctx, "menu_export");
        click_action(&mut dialog, &ctx, "export_mermaid");
        assert_eq!(std::fs::read(&occupied_path).unwrap(), occupied_bytes);
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Error(message)) if message.contains("occupied")
        ));
        assert_eq!(dialog.test_file_picker_calls(), 3);
        assert_eq!(dialog.test_session().draft(), original_source);
        assert_eq!(dialog.test_session().revision(), original_revision);
        assert_eq!(dialog.test_session().undo_depth(), original_undo);
        assert_eq!(dialog.test_session().redo_depth(), original_redo);
        assert_eq!(dialog.documents.is_dirty(), original_dirty);
        assert_eq!(dialog.documents.path(), original_path.as_deref());
        assert_eq!(dialog.documents.disk_digest(), original_digest);
    }

    #[test]
    fn diagram_studio_stale_and_pending_states_block_image_and_document_exports() {
        let directory = tempfile::tempdir().expect("temporary export directory");
        let stale_path = directory.path().join("stale.svg");
        let mut stale = initialized_dialog();
        stale.test_replace_source("flowchart TD\nA -->\n");
        let stale_ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut stale, &stale_ctx);
        }
        stale.test_set_next_file_picker_result(Some(stale_path.clone()));
        stale.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        frame(&mut stale, &stale_ctx);
        assert_eq!(stale.test_file_picker_calls(), 0);
        assert!(!stale_path.exists());
        assert_eq!(stale.test_session().status().preview, PreviewState::Stale);
        assert!(matches!(
            &stale.message,
            Some(StudioNotice::Error(message)) if message.contains("preview")
        ));

        let pending_path = directory.path().join("pending.mmd");
        let mut pending = initialized_dialog();
        let pending_ctx = egui::Context::default();
        assert_eq!(
            pending.documents.request_new().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        pending.test_set_next_file_picker_result(Some(pending_path.clone()));
        pending.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Mermaid,
            ExportScale::One,
        ));
        frame(&mut pending, &pending_ctx);
        assert_eq!(pending.test_file_picker_calls(), 0);
        assert!(!pending_path.exists());
        assert_eq!(
            pending.documents.pending_transition(),
            Some(&PendingDiagramTransition::New)
        );
        assert!(pending.documents.is_dirty());
        assert!(matches!(
            &pending.message,
            Some(StudioNotice::Error(message)) if message.contains("pending document action")
        ));
    }

    #[test]
    fn diagram_studio_visual_authoring_template_uses_accepted_dirty_transition() {
        let mut dialog = initialized_visual_dialog();
        dialog
            .documents
            .replace_visual_label("Current visual")
            .unwrap();
        dialog.refresh_cache();
        let original_document = dialog.render_document.clone();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        click_action(&mut dialog, &ctx, "menu_visual_templates");
        for _ in 0..2 {
            frame(&mut dialog, &ctx);
        }
        for template in crate::diagram::VisualAuthoringTemplate::ALL {
            let action = format!("visual_authoring_template_{}", template.slug());
            assert!(
                dialog
                    .test_action_rects
                    .iter()
                    .any(|(name, rect, clip)| name == &action && clip.contains_rect(*rect)),
                "missing {action}"
            );
            assert_eq!(
                dialog.test_action_enabled(&action),
                Some(true),
                "{action} should remain enabled inside its own menu"
            );
        }
        click_action(&mut dialog, &ctx, "visual_authoring_template_architecture");
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::ApplyVisualTemplate(
                crate::diagram::VisualAuthoringTemplate::Architecture
            )),
            "accepted visual template action failed: {:?}",
            dialog.message
        );
        reveal_visual_action(&mut dialog, &ctx, "decision_cancel");
        click_action(&mut dialog, &ctx, "decision_cancel");
        assert_eq!(dialog.documents.visual_label(), Some("Current visual"));
        assert_eq!(dialog.render_document, original_document);
        assert!(dialog.documents.is_dirty());

        reveal_visual_action(&mut dialog, &ctx, "menu_visual_templates");
        click_action(&mut dialog, &ctx, "menu_visual_templates");
        click_action(&mut dialog, &ctx, "visual_authoring_template_architecture");
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::ApplyVisualTemplate(
                crate::diagram::VisualAuthoringTemplate::Architecture
            ))
        );
        reveal_visual_action(&mut dialog, &ctx, "decision_discard");
        click_action(&mut dialog, &ctx, "decision_discard");
        assert_eq!(
            dialog.documents.visual_label(),
            Some("Architecture starter")
        );
        assert_eq!(
            dialog.documents.authoring_kind(),
            Some(crate::diagram::DiagramAuthoringKind::Visual)
        );
        assert_ne!(dialog.render_document, original_document);
        assert!(dialog.documents.is_dirty());
    }

    #[test]
    fn diagram_studio_visual_template_menu_keyboard_selects_once() {
        let mut dialog = initialized_visual_dialog();
        let old = dialog.render_document.clone();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        open_menu_with_keyboard(
            &mut dialog,
            &ctx,
            "menu_visual_templates",
            "visual_authoring_template_flowchart",
        );
        send_child_key(
            &mut dialog,
            &ctx,
            egui::Key::ArrowDown,
            egui::Modifiers::NONE,
        );
        assert!(dialog.test_action_has_focus("visual_authoring_template_architecture"));
        send_child_key(&mut dialog, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
        assert_ne!(dialog.render_document, old);
        assert_eq!(
            dialog.documents.visual_label(),
            Some("Architecture starter")
        );
        assert!(dialog.documents.pending_transition().is_none());
    }

    #[test]
    fn diagram_studio_visual_templates_all_create_editable_starters_once() {
        for template in crate::diagram::VisualAuthoringTemplate::ALL {
            let mut dialog = initialized_visual_dialog();
            let original_identity = dialog.render_document.clone();
            let ctx = egui::Context::default();
            for _ in 0..3 {
                frame(&mut dialog, &ctx);
            }
            click_action(&mut dialog, &ctx, "menu_visual_templates");
            for _ in 0..2 {
                frame(&mut dialog, &ctx);
            }
            let action = format!("visual_authoring_template_{}", template.slug());
            assert_eq!(dialog.test_action_enabled(&action), Some(true));
            click_action(&mut dialog, &ctx, &action);
            assert!(dialog.documents.pending_transition().is_none());
            assert_ne!(dialog.render_document, original_identity);
            let expected = template.create_document().unwrap();
            assert_eq!(dialog.documents.visual_scene(), Some(expected.scene()));
            assert!(dialog.documents.is_dirty());
            let revision = dialog.documents.visual_revision().unwrap();
            click_action(&mut dialog, &ctx, "visual_add_shape");
            let edited = dialog.documents.visual_scene().unwrap().clone();
            assert_eq!(dialog.documents.visual_revision(), Some(revision + 1));
            click_action(&mut dialog, &ctx, "visual_undo");
            assert_eq!(dialog.documents.visual_scene(), Some(expected.scene()));
            click_action(&mut dialog, &ctx, "visual_redo");
            assert_eq!(dialog.documents.visual_scene(), Some(&edited));
        }
    }

    #[test]
    fn diagram_studio_visual_template_discarded_root_and_decisions_preserve_state() {
        let mut dialog = initialized_visual_dialog();
        dialog.documents.replace_visual_label("Original").unwrap();
        let original_scene = dialog.documents.visual_scene().unwrap().clone();
        let original_identity = dialog.render_document.clone();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        click_action(&mut dialog, &ctx, "menu_visual_templates");
        click_action_with_root_discard(&mut dialog, &ctx, "visual_authoring_template_architecture");
        assert!(dialog.documents.pending_transition().is_none());
        assert_eq!(dialog.documents.visual_scene(), Some(&original_scene));
        assert_eq!(dialog.render_document, original_identity);
        click_action(&mut dialog, &ctx, "menu_visual_templates");
        click_action(&mut dialog, &ctx, "visual_authoring_template_architecture");
        let pending = dialog.documents.pending_transition().cloned();
        assert!(matches!(
            pending,
            Some(PendingDiagramTransition::ApplyVisualTemplate(
                crate::diagram::VisualAuthoringTemplate::Architecture
            ))
        ));
        reveal_visual_action(&mut dialog, &ctx, "decision_discard");
        click_action_with_child_discard(&mut dialog, &ctx, "decision_discard");
        assert_eq!(dialog.documents.pending_transition(), pending.as_ref());
        assert_eq!(dialog.documents.visual_scene(), Some(&original_scene));
        assert_eq!(dialog.render_document, original_identity);
        click_action_with_root_discard(&mut dialog, &ctx, "decision_discard");
        assert_eq!(dialog.documents.pending_transition(), pending.as_ref());
        assert_eq!(dialog.documents.visual_scene(), Some(&original_scene));
        assert_eq!(dialog.render_document, original_identity);
    }

    #[test]
    fn diagram_studio_visual_authoring_template_discarded_child_pass_keeps_current_document() {
        let mut dialog = initialized_visual_dialog();
        dialog
            .documents
            .replace_visual_label("Current visual")
            .unwrap();
        dialog.refresh_cache();
        let original_document = dialog.render_document.clone();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        click_action(&mut dialog, &ctx, "menu_visual_templates");
        click_action_with_child_discard(
            &mut dialog,
            &ctx,
            "visual_authoring_template_architecture",
        );

        assert!(dialog.test_child_discard_requested);
        assert!(dialog.documents.pending_transition().is_none());
        assert_eq!(dialog.documents.visual_label(), Some("Current visual"));
        assert_eq!(dialog.render_document, original_document);
        assert!(dialog.documents.is_dirty());
    }

    #[test]
    fn diagram_studio_template_discard_replaces_equal_revision_and_resets_presentation() {
        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        dialog.test_replace_source("flowchart TD\nA[\"Old graph\"]\n");
        let revision = dialog.test_session().revision();
        assert_eq!(revision, 1);
        let node_id = NodeId::new("A").unwrap();
        dialog.set_selection(DiagramElement::Node(node_id.clone()), false);
        dialog.property_draft = Some(PropertyDraft {
            target: DiagramElement::Node(node_id),
            basis_revision: revision,
            fields: PropertyFields::Node {
                label: "pending edit".into(),
                shape: NodeShape::Rectangle,
                group: None,
            },
        });
        dialog.camera = Camera {
            center: ScenePoint {
                x: 900.0,
                y: -400.0,
            },
            zoom: 3.0,
            fit_next: false,
        };
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        click_action(&mut dialog, &ctx, "menu_templates");
        for name in ["template_linear", "template_decision", "template_process"] {
            assert!(
                dialog
                    .test_action_rects
                    .iter()
                    .any(|(action, _, _)| action == name)
            );
        }
        click_action(&mut dialog, &ctx, "template_linear");
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::ApplyTemplate(
                DiagramTemplate::Linear
            ))
        );
        click_action(&mut dialog, &ctx, "decision_discard");

        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(
            dialog.test_session().draft(),
            DiagramTemplate::Linear.source()
        );
        assert_eq!(
            dialog.test_session().status().preview,
            PreviewState::Current
        );
        assert_eq!(dialog.documents.path(), None);
        assert_eq!(dialog.documents.disk_digest(), None);
        assert!(dialog.selection.is_none());
        assert!(dialog.selected_nodes.is_empty());
        assert!(dialog.property_draft.is_none());
        assert!(dialog.drag.is_none());
        assert_eq!(dialog.camera.zoom, 1.0);
        assert_eq!(dialog.camera.center, ScenePoint::default());
        assert!(dialog.camera.fit_next);
        let cache = dialog.cache.as_ref().unwrap();
        assert_eq!(cache.revision, revision);
        let scene = cache.scene.as_ref().unwrap();
        assert_eq!(scene.nodes.len(), 3);
        assert!(scene.nodes.iter().any(|node| node.id.as_str() == "Start"));
        assert!(!scene.nodes.iter().any(|node| node.id.as_str() == "A"));
    }

    #[test]
    fn diagram_studio_auto_layout_is_one_undoable_edit_and_repeat_is_noop() {
        let mut dialog = initialized_dialog();
        dialog.test_replace_source("flowchart LR\nA --> B\nA --> C\nC --> D\n");
        let overlapping_positions = dialog
            .test_session()
            .current_graph()
            .unwrap()
            .nodes
            .iter()
            .map(|node| NodePosition {
                id: node.id.clone(),
                position: Position::new(500.0, 500.0).unwrap(),
            })
            .collect::<Vec<_>>();
        dialog
            .documents
            .apply_transaction(&[GraphOperation::MoveNodes {
                positions: overlapping_positions,
            }])
            .unwrap();
        dialog.refresh_cache();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let original_positions = current_node_positions(&dialog);
        let revision = dialog.test_session().revision();
        let undo_depth = dialog.test_session().undo_depth();

        click_action(&mut dialog, &ctx, "auto_layout");
        let layout_positions = current_node_positions(&dialog);
        assert_ne!(layout_positions, original_positions);
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth + 1);

        click_action(&mut dialog, &ctx, "auto_layout");
        assert_eq!(current_node_positions(&dialog), layout_positions);
        assert_eq!(dialog.test_session().revision(), revision + 1);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth + 1);

        dialog.test_pending_mutation = Some(PendingMutation::Undo);
        frame(&mut dialog, &ctx);
        assert_eq!(current_node_positions(&dialog), original_positions);
        assert!(dialog.test_session().can_redo());
    }

    #[test]
    fn diagram_studio_deleted_native_export_identity_normalizes_windows_path_spellings() {
        let current_dir = std::env::current_dir().expect("resolve test working directory");
        let test_root = current_dir.join("target/visual-studios");
        std::fs::create_dir_all(&test_root).expect("create owned test evidence directory");
        let directory = tempfile::Builder::new()
            .prefix("diagram-export-path-")
            .tempdir_in(test_root)
            .expect("create path identity fixture");
        let active_path = directory.path().join("Native.mld");
        assert!(!active_path.exists(), "the compared native path is deleted");

        let dot_alias = active_path.parent().unwrap().join(".").join("Native.mld");
        assert!(same_export_destination(&active_path, &dot_alias).unwrap());
        let relative = active_path
            .strip_prefix(&current_dir)
            .expect("fixture is under the repository root")
            .to_path_buf();
        assert!(same_export_destination(&active_path, &relative).unwrap());

        #[cfg(windows)]
        {
            let ascii_case_variant = PathBuf::from(active_path.to_string_lossy().to_uppercase());
            let verbatim = PathBuf::from(format!(r"\\?\{}", active_path.display()));
            let unicode_upper_sigma = directory.path().join("sigma\u{03a3}.mld");
            let unicode_lower_sigma = directory.path().join("sigma\u{03c3}.mld");
            let unicode_final_sigma = directory.path().join("sigma\u{03c2}.mld");
            assert!(same_export_destination(&active_path, &ascii_case_variant).unwrap());
            assert!(same_export_destination(&active_path, &verbatim).unwrap());
            assert!(
                same_export_destination(&unicode_upper_sigma, &unicode_lower_sigma).unwrap(),
                "Windows ordinal case-insensitive comparison folds capital and ordinary sigma"
            );
            assert!(
                !same_export_destination(&unicode_upper_sigma, &unicode_final_sigma).unwrap(),
                "the final sigma remains a distinct Windows filename"
            );

            let parent_alias = directory.path().with_extension("alias");
            if std::os::windows::fs::symlink_dir(directory.path(), &parent_alias).is_ok() {
                assert!(
                    same_export_destination(&active_path, &parent_alias.join("Native.mld"))
                        .unwrap()
                );
                std::fs::remove_dir(parent_alias).expect("remove temporary parent alias");
            }
        }
    }

    #[test]
    fn diagram_studio_export_refuses_deleted_active_native_path_without_state_change() {
        let current_dir = std::env::current_dir().expect("resolve test working directory");
        let test_root = current_dir.join("target/visual-studios");
        std::fs::create_dir_all(&test_root).expect("create owned test evidence directory");
        let directory = tempfile::Builder::new()
            .prefix("diagram-export-native-path-")
            .tempdir_in(test_root)
            .expect("create native path fixture");
        let native_path = directory.path().join("native.mld");
        let mut dialog = initialized_dialog();
        dialog
            .documents
            .save_as(&native_path)
            .expect("establish active native identity");
        dialog.test_replace_source("flowchart TD\nA[\"Unsaved edit\"]\n");
        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();
        let undo_depth = dialog.test_session().undo_depth();
        let disk_digest = dialog.documents.disk_digest();
        assert!(dialog.documents.is_dirty());
        std::fs::remove_file(&native_path).expect("remove active file while keeping identity");

        let relative = native_path
            .strip_prefix(&current_dir)
            .expect("fixture is under the repository root");
        let selected_path = relative
            .parent()
            .unwrap()
            .join(".")
            .join(relative.file_name().unwrap());
        dialog.test_set_next_file_picker_result(Some(selected_path));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);

        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert!(!native_path.exists());
        assert_eq!(dialog.documents.path(), Some(native_path.as_path()));
        assert_eq!(dialog.documents.disk_digest(), disk_digest);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert!(dialog.documents.is_dirty());
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Error(message)) if message.contains("active native Diagram document")
        ));
    }

    #[test]
    fn studio_usability_diagram_viewer_uses_a_distinct_callback_and_independent_camera() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        dialog.camera = Camera {
            center: ScenePoint { x: 18.0, y: -9.0 },
            zoom: 1.3,
            fit_next: false,
        };
        dialog.set_selection(
            DiagramElement::Node(NodeId::new("A").expect("fixture node ID is valid")),
            false,
        );
        frame(&mut dialog, &ctx);

        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();
        let dirty = dialog.documents.is_dirty();
        let undo_depth = dialog.test_session().undo_depth();
        let redo_depth = dialog.test_session().redo_depth();
        let selection = dialog.selection.clone();
        let editor_camera = dialog.camera;
        let scene_builds = dialog.test_scene_build_count;
        let editor_viewport = dialog.test_last_callback().0.expect("editor callback ran");

        open_viewer(&mut dialog, &ctx);

        let (viewer_callback, class, body_rendered) = dialog.test_last_viewer_callback();
        assert_eq!(editor_viewport, viewport_id());
        assert_eq!(viewer_callback, Some(viewer_viewport_id()));
        assert_ne!(viewer_viewport_id(), viewport_id());
        assert!(matches!(class, Some(egui::ViewportClass::Immediate)));
        assert!(body_rendered);
        assert_eq!(dialog.test_last_callback().0, Some(viewport_id()));
        assert!(dialog.viewer_open);
        assert_eq!(dialog.camera.center, editor_camera.center);
        assert_eq!(dialog.camera.zoom, editor_camera.zoom);
        assert_eq!(dialog.camera.fit_next, editor_camera.fit_next);

        click_viewer_canvas(&mut dialog, &ctx);
        assert!(dialog.test_viewer_canvas_has_focus());
        let viewer_camera_before_keys = dialog.viewer_camera;
        send_viewer_key(
            &mut dialog,
            &ctx,
            egui::Key::ArrowRight,
            egui::Modifiers::COMMAND,
        );
        send_viewer_key(
            &mut dialog,
            &ctx,
            egui::Key::Minus,
            egui::Modifiers::COMMAND,
        );

        assert_ne!(
            dialog.viewer_camera.center,
            viewer_camera_before_keys.center
        );
        assert_ne!(dialog.viewer_camera.zoom, viewer_camera_before_keys.zoom);
        assert_eq!(dialog.camera.center, editor_camera.center);
        assert_eq!(dialog.camera.zoom, editor_camera.zoom);
        assert_eq!(dialog.camera.fit_next, editor_camera.fit_next);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.documents.is_dirty(), dirty);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert_eq!(dialog.test_session().redo_depth(), redo_depth);
        assert_eq!(dialog.selection, selection);
        assert_eq!(dialog.test_scene_build_count, scene_builds);
    }

    #[test]
    fn studio_usability_diagram_viewer_discard_close_and_reopen_are_generation_scoped() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        click_action_with_child_discard(&mut dialog, &ctx, "open_viewer");
        assert!(
            !dialog.viewer_open,
            "a discarded owner child cannot open the viewer"
        );

        click_action_with_root_discard(&mut dialog, &ctx, "open_viewer");
        assert!(
            !dialog.viewer_open,
            "a discarded ROOT pass cannot open the viewer"
        );

        open_viewer(&mut dialog, &ctx);
        let original_key = dialog
            .test_viewer_render_key
            .clone()
            .expect("viewer records the current native render key");
        assert_eq!(
            original_key.accepted_revision,
            dialog.test_session().revision()
        );

        // Repeated Open viewer requests focus the same native viewport.
        click_action(&mut dialog, &ctx, "open_viewer");
        frame(&mut dialog, &ctx);
        assert!(dialog.viewer_open);
        assert_eq!(
            dialog.test_last_viewer_callback().0,
            Some(viewer_viewport_id())
        );
        assert_eq!(dialog.test_viewer_render_key.as_ref(), Some(&original_key));

        dialog.test_request_viewer_child_discard();
        let _ = viewer_close_request_commands(&mut dialog, &ctx);
        assert!(
            dialog.viewer_open,
            "a discarded viewer child cannot retire the viewer"
        );

        let _ = viewer_close_request_commands_with_root_discard(&mut dialog, &ctx);
        assert!(
            dialog.viewer_open,
            "a discarded ROOT pass cannot retire the viewer"
        );

        let _ = viewer_close_request_commands(&mut dialog, &ctx);
        assert!(
            !dialog.viewer_open,
            "an accepted viewer X closes only that window"
        );
        assert!(dialog.open, "the owning editor remains open");
        assert_eq!(
            dialog.test_session().revision(),
            original_key.accepted_revision
        );
        frame(&mut dialog, &ctx);
        assert!(
            !dialog.viewer_open,
            "an accepted close is not replayed on later frames"
        );

        open_viewer(&mut dialog, &ctx);
        assert!(dialog.viewer_open);
        assert_eq!(
            dialog.test_last_viewer_callback().0,
            Some(viewer_viewport_id())
        );
        assert_eq!(dialog.test_viewer_render_key.as_ref(), Some(&original_key));
        assert_eq!(
            dialog.documents.session().unwrap().revision(),
            original_key.accepted_revision
        );
    }

    #[test]
    fn studio_usability_diagram_viewer_owner_dirty_decisions_keep_or_retire_viewer() {
        let ctx = egui::Context::default();

        let mut cancel_dialog = initialized_dialog();
        open_viewer(&mut cancel_dialog, &ctx);
        cancel_dialog.test_replace_source("flowchart TD\n    A[\"Cancel retains viewer\"]");
        let cancel_source = cancel_dialog.test_session().draft().to_owned();
        let cancel_revision = cancel_dialog.test_session().revision();
        cancel_dialog.request_close();
        frame(&mut cancel_dialog, &ctx);
        assert_eq!(
            cancel_dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        click_action(&mut cancel_dialog, &ctx, "decision_cancel");
        assert!(cancel_dialog.open);
        assert!(cancel_dialog.viewer_open);
        assert_eq!(cancel_dialog.test_session().draft(), cancel_source);
        assert_eq!(cancel_dialog.test_session().revision(), cancel_revision);
        assert!(cancel_dialog.documents.is_dirty());

        let mut discard_dialog = initialized_dialog();
        open_viewer(&mut discard_dialog, &ctx);
        discard_dialog.test_replace_source("flowchart TD\n    A[\"Discard closes owner\"]");
        discard_dialog.request_close();
        frame(&mut discard_dialog, &ctx);
        click_action(&mut discard_dialog, &ctx, "decision_discard");
        assert!(!discard_dialog.open);
        assert!(!discard_dialog.viewer_open);

        let directory = tempfile::tempdir().expect("temporary viewer save directory");
        let saved_path = directory.path().join("viewer-close.mld");
        let mut save_dialog = initialized_dialog();
        save_dialog
            .documents
            .save_as(&saved_path)
            .expect("establish a clean native baseline before editing");
        open_viewer(&mut save_dialog, &ctx);
        let saved_source = "flowchart TD\n    A[\"Save closes owner\"]";
        save_dialog.test_replace_source(saved_source);
        assert!(save_dialog.documents.is_dirty());
        save_dialog.request_close();
        frame(&mut save_dialog, &ctx);
        click_action(&mut save_dialog, &ctx, "decision_save");
        assert!(!save_dialog.open);
        assert!(!save_dialog.viewer_open);
        assert!(!save_dialog.documents.is_dirty());
        assert!(
            std::fs::read(&saved_path)
                .expect("Save decision wrote the document")
                .windows(b"Save closes owner".len())
                .any(|window| window == b"Save closes owner")
        );
    }

    #[test]
    fn diagram_owner_dock_and_pop_out_keep_the_same_document_generation() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        let owner = dialog.owner_generation;
        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();

        click_action(&mut dialog, &ctx, "owner_presentation");
        assert_eq!(
            dialog.owner_host.presentation(),
            UtilityPresentation::Docked
        );
        assert!(dialog.take_show_root_after_host_change());
        assert!(!dialog.take_show_root_after_host_change());

        click_root_action(
            &mut dialog,
            &ctx,
            "owner_presentation",
            diagram_host_layer("diagram-studio"),
        );
        assert_eq!(
            dialog.owner_host.presentation(),
            UtilityPresentation::Popout
        );
        assert!(dialog.open);
        assert_eq!(dialog.owner_generation, owner);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
    }

    #[test]
    fn focused_docked_viewer_owns_root_close_shortcuts_from_docked_owner() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        click_action(&mut dialog, &ctx, "owner_presentation");
        assert_eq!(
            dialog.owner_host.presentation(),
            UtilityPresentation::Docked
        );

        click_root_action(
            &mut dialog,
            &ctx,
            "open_viewer",
            diagram_host_layer("diagram-studio"),
        );
        assert!(dialog.viewer_open);
        click_viewer_action(&mut dialog, &ctx, "viewer_presentation");
        assert_eq!(
            dialog.viewer_host.presentation(),
            UtilityPresentation::Docked
        );

        frame(&mut dialog, &ctx);
        frame(&mut dialog, &ctx);
        let viewer_canvas = dialog.test_viewer_canvas_id.expect("docked viewer canvas");
        let click = dialog
            .test_viewer_canvas_rect
            .expect("docked canvas rect")
            .center();
        assert_eq!(
            ctx.layer_id_at(click),
            Some(diagram_host_layer("diagram-viewer"))
        );
        click_root_position(&mut dialog, &ctx, click);
        frame(&mut dialog, &ctx);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(viewer_canvas));
        assert!(dialog.embedded_viewer_owns_root_input(&ctx));

        for (key, modifiers) in [
            (egui::Key::W, egui::Modifiers::COMMAND),
            (egui::Key::Escape, egui::Modifiers::NONE),
        ] {
            for pressed in [true, false] {
                let mut events = vec![
                    egui::Event::ModifiersChanged(modifiers),
                    egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers,
                    },
                ];
                if !pressed {
                    events.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
                }
                frame_with_root_events(&mut dialog, &ctx, events);
                assert!(dialog.open, "the docked viewer owns {key:?}");
            }
        }
        assert!(dialog.viewer_open);
        // Clicking away from the focused Viewer must release its root ownership.
        click_root_position(&mut dialog, &ctx, egui::pos2(1196.0, 896.0));
        assert!(!dialog.embedded_viewer_owns_root_input(&ctx));

        let viewer_generation = dialog.viewer_generation;
        let camera = dialog.viewer_camera;
        click_root_action(
            &mut dialog,
            &ctx,
            "viewer_presentation",
            diagram_host_layer("diagram-viewer"),
        );
        assert_eq!(
            dialog.viewer_host.presentation(),
            UtilityPresentation::Popout
        );
        assert_eq!(dialog.viewer_generation, viewer_generation);
        assert_eq!(dialog.viewer_camera.center, camera.center);
        assert_eq!(dialog.viewer_camera.zoom, camera.zoom);
    }

    #[test]
    fn diagram_hidden_root_skips_docked_owner_but_keeps_native_viewer() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        open_viewer(&mut dialog, &ctx);
        click_action(&mut dialog, &ctx, "owner_presentation");
        assert_eq!(
            dialog.owner_host.presentation(),
            UtilityPresentation::Docked
        );
        assert_eq!(
            dialog.viewer_host.presentation(),
            UtilityPresentation::Popout
        );
        assert!(dialog.needs_root_ui_when_hidden());

        super::super::render::with_immediate_viewport_renderer(&ctx, || {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        TEST_VIEWPORT_SIZE,
                    )),
                    focused: false,
                    ..Default::default()
                },
                |ui| {
                    dialog.show_on_root(ui.ctx(), ui.current_pass_index() == 0, true, false, false)
                },
            );
            output.drop_without_applying_deltas();
            dialog.on_ui_frame_complete(&ctx);
        });
        assert!(!dialog.test_body_rendered);
        assert!(dialog.test_viewer_body_rendered);
        assert!(dialog.open && dialog.viewer_open);
    }

    #[test]
    fn diagram_visual_host_discards_unretained_dock_click() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();

        click_action_with_root_discard(&mut dialog, &ctx, "owner_presentation");
        assert_eq!(
            dialog.owner_host.presentation(),
            UtilityPresentation::Popout
        );
        assert!(!dialog.take_show_root_after_host_change());

        click_action(&mut dialog, &ctx, "owner_presentation");
        assert_eq!(
            dialog.owner_host.presentation(),
            UtilityPresentation::Docked
        );
        assert!(dialog.take_show_root_after_host_change());
        assert_eq!(
            dialog.documents.authoring_kind(),
            Some(crate::diagram::DiagramAuthoringKind::Visual),
        );
    }

    #[test]
    fn studio_usability_diagram_viewer_same_revision_document_switch_updates_identity() {
        let directory = tempfile::tempdir().expect("temporary viewer document directory");
        let path_a = directory.path().join("viewer-a.mld");
        let path_b = directory.path().join("viewer-b.mld");
        for (path, source) in [
            (&path_a, "flowchart TD\n    A[\"Viewer A\"]"),
            (&path_b, "flowchart LR\n    B[\"Viewer B\"]"),
        ] {
            let mut document = DiagramDocumentController::new();
            document
                .replace_source(source)
                .expect("native fixture parses");
            assert_eq!(document.session().unwrap().revision(), 1);
            document.save_as(path).expect("save native fixture");
        }

        let mut dialog = DiagramStudioDialog::default();
        dialog.open();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        dialog.test_set_next_file_picker_result(Some(path_a.clone()));
        click_action(&mut dialog, &ctx, "document_open");
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.documents.path(), Some(path_a.as_path()));
        assert_eq!(dialog.test_session().revision(), 1);
        open_viewer(&mut dialog, &ctx);
        let key_a = dialog
            .test_viewer_render_key
            .clone()
            .expect("viewer rendered document A");

        dialog.test_set_next_file_picker_result(Some(path_b.clone()));
        click_action(&mut dialog, &ctx, "document_open");
        frame(&mut dialog, &ctx);

        let key_b = dialog
            .test_viewer_render_key
            .clone()
            .expect("viewer rendered document B");
        assert_eq!(dialog.documents.path(), Some(path_b.as_path()));
        assert_eq!(dialog.test_session().revision(), 1);
        assert_eq!(key_a.accepted_revision, key_b.accepted_revision);
        assert_ne!(key_a.document, key_b.document);
        assert_eq!(key_b.document, dialog.render_document);
        let scene = dialog
            .cache
            .as_ref()
            .and_then(|cache| cache.scene.as_ref())
            .expect("document B has a shared native scene");
        assert_eq!(scene.nodes[0].id.as_str(), "B");
        assert_eq!(dialog.viewer_camera.fit_next, false);
        assert!(dialog.viewer_open);
        assert_eq!(
            dialog.test_session().draft(),
            "flowchart LR\n    B[\"Viewer B\"]"
        );
    }

    #[test]
    fn studio_usability_diagram_viewer_generic_resize_and_camera_share_one_render_result() {
        let source = "mindmap\n  root((Viewer render))\n    Shared result";
        let mut dialog = generic_dialog(source);
        let ctx = egui::Context::default();
        let _output = wait_for_generic_preview(&mut dialog, &ctx);
        let revision = dialog.test_session().revision();
        let source_before = dialog.test_session().draft().to_owned();
        let undo_depth = dialog.test_session().undo_depth();
        let redo_depth = dialog.test_session().redo_depth();
        let counts = dialog
            .viewer_render_counters_for_test()
            .expect("generic render service exists");

        open_viewer(&mut dialog, &ctx);
        assert_eq!(dialog.viewer_render_counters_for_test(), Some(counts));
        assert_eq!(
            dialog.test_viewer_render_key.as_ref(),
            dialog.current_generic_key().as_ref()
        );
        frame_with_viewer_size(&mut dialog, &ctx, egui::vec2(1040.0, 760.0));
        assert_eq!(dialog.viewer_render_counters_for_test(), Some(counts));

        click_viewer_canvas(&mut dialog, &ctx);
        send_viewer_key(
            &mut dialog,
            &ctx,
            egui::Key::ArrowDown,
            egui::Modifiers::COMMAND,
        );
        send_viewer_key(&mut dialog, &ctx, egui::Key::Plus, egui::Modifiers::COMMAND);
        assert_eq!(dialog.viewer_render_counters_for_test(), Some(counts));
        assert!(dialog.test_generic_preview_drawn());
        assert_eq!(dialog.test_session().draft(), source_before);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert_eq!(dialog.test_session().redo_depth(), redo_depth);
        assert_eq!(
            dialog.test_viewer_render_key.as_ref(),
            dialog.current_generic_key().as_ref()
        );
    }

    #[test]
    fn studio_usability_diagram_viewer_bootstraps_when_root_input_is_hidden() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        click_action(&mut dialog, &ctx, "open_viewer");
        assert!(dialog.viewer_open);
        assert!(dialog.viewer_bootstrap_pending);
        assert!(dialog.needs_root_ui_when_hidden());

        frame_with_root_input(&mut dialog, &ctx, false);

        assert_eq!(
            dialog.test_last_viewer_callback().0,
            Some(viewer_viewport_id())
        );
        assert!(dialog.test_last_viewer_callback().2);
        assert!(!dialog.viewer_bootstrap_pending);
        assert!(dialog.open);
    }

    #[test]
    fn studio_usability_diagram_viewer_embedded_fallback_keeps_root_host_identity() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(true);
        for _ in 0..3 {
            embedded_frame_with_events(&mut dialog, &ctx, Vec::new(), false);
        }

        assert_ne!(viewer_viewport_id(), viewport_id());
        click_action_in_embedded_fallback(&mut dialog, &ctx, "open_viewer");
        assert!(dialog.viewer_open);
        for _ in 0..3 {
            embedded_frame_with_events(&mut dialog, &ctx, Vec::new(), true);
        }

        let (callback_id, callback_class, body_rendered) = dialog.test_last_viewer_callback();
        assert_eq!(callback_id, Some(egui::ViewportId::ROOT));
        assert!(matches!(
            callback_class,
            Some(egui::ViewportClass::EmbeddedWindow)
        ));
        assert!(body_rendered);
        assert!(dialog.embedded_viewer_layer_if_root().is_some());
        assert!(dialog.open);
        assert!(dialog.viewer_open);
        assert_eq!(
            dialog.test_session().draft(),
            "flowchart TD\n    A[\"Alpha\"]"
        );

        let viewer_canvas = dialog
            .test_viewer_canvas_rect
            .expect("embedded viewer canvas was drawn");
        let click = viewer_canvas.center();
        for pressed in [true, false] {
            embedded_frame_with_events(
                &mut dialog,
                &ctx,
                vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                true,
            );
        }
        for _ in 0..2 {
            embedded_frame_with_events(&mut dialog, &ctx, Vec::new(), true);
        }
        let viewer_focus = dialog.viewer_embedded_root_focused_id;
        let viewer_focus = viewer_focus.expect("accepted ROOT canvas focus has a viewer widget ID");
        assert_eq!(dialog.test_viewer_canvas_id, Some(viewer_focus));
        let source = dialog.test_session().draft().to_owned();
        let revision = dialog.test_session().revision();
        let editor_camera = dialog.camera;
        let viewer_camera = dialog.viewer_camera;
        let undo_depth = dialog.test_session().undo_depth();
        let redo_depth = dialog.test_session().redo_depth();
        assert!(dialog.embedded_viewer_owns_root_input(&ctx));

        send_embedded_fallback_key(
            &mut dialog,
            &ctx,
            egui::Key::ArrowRight,
            egui::Modifiers::COMMAND,
        );
        assert_ne!(dialog.viewer_camera.center, viewer_camera.center);
        assert_eq!(dialog.camera.center, editor_camera.center);
        assert_eq!(dialog.camera.zoom, editor_camera.zoom);
        assert_eq!(dialog.test_session().draft(), source);
        assert_eq!(dialog.test_session().revision(), revision);
        assert_eq!(dialog.test_session().undo_depth(), undo_depth);
        assert_eq!(dialog.test_session().redo_depth(), redo_depth);

        let directory = tempfile::tempdir().expect("temporary embedded save path");
        let save_target = directory.path().join("viewer-shortcut-save.mld");
        dialog.test_set_next_file_picker_result(Some(save_target.clone()));
        let file_picker_calls = dialog.test_file_picker_calls();
        send_embedded_fallback_key(&mut dialog, &ctx, egui::Key::S, egui::Modifiers::COMMAND);
        assert_eq!(dialog.test_file_picker_calls(), file_picker_calls);
        assert_eq!(dialog.documents.path(), None);
        assert!(
            !save_target.exists(),
            "viewer Cmd+S does not save the owner"
        );

        send_embedded_fallback_key(&mut dialog, &ctx, egui::Key::W, egui::Modifiers::COMMAND);
        assert!(dialog.open, "viewer Cmd+W does not close the owner editor");
        assert!(dialog.viewer_open);
    }

    #[test]
    fn diagram_studio_authoring_visual_intent_is_accepted_and_retires_equal_revision_preview() {
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        dialog.view_mode = ViewMode::Source {
            restore: PreviewMode::Visual,
        };
        open_viewer(&mut dialog, &ctx);
        let old_document_identity = dialog.render_document.clone();
        let old_revision = dialog.documents.session().unwrap().revision();
        assert_eq!(old_revision, 1);
        assert!(dialog.test_viewer_render_key.is_some());

        click_action_with_child_discard(&mut dialog, &ctx, "document_new_visual");
        assert_eq!(
            dialog.documents.authoring_kind(),
            Some(crate::diagram::DiagramAuthoringKind::Mermaid)
        );
        assert_eq!(dialog.documents.session().unwrap().revision(), old_revision);
        assert_eq!(dialog.render_document, old_document_identity);

        click_action(&mut dialog, &ctx, "document_new_visual");
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::NewVisual)
        );
        assert_eq!(
            dialog.documents.authoring_kind(),
            Some(crate::diagram::DiagramAuthoringKind::Mermaid)
        );
        assert_eq!(dialog.render_document, old_document_identity);

        click_action(&mut dialog, &ctx, "decision_discard");
        assert_eq!(
            dialog.documents.authoring_kind(),
            Some(crate::diagram::DiagramAuthoringKind::Visual)
        );
        assert!(dialog.documents.has_active_document());
        assert!(dialog.documents.session().is_none());
        assert_ne!(dialog.render_document, old_document_identity);
        assert!(dialog.viewer_open);
        assert_eq!(
            dialog.view_mode,
            ViewMode::Source {
                restore: PreviewMode::Visual,
            }
        );

        dialog
            .documents
            .replace_visual_label("Visual at revision one")
            .unwrap();
        assert_eq!(dialog.documents.visual_revision(), Some(old_revision));
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_viewer_render_key, None);
        assert_eq!(
            dialog.documents.visual_label(),
            Some("Visual at revision one")
        );
        assert!(dialog.test_action_rect("visual_label").is_some());
    }

    #[test]
    fn diagram_studio_authoring_visual_text_undo_does_not_cross_document_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("visual.mld");
        let mut dialog = DiagramStudioDialog::default();
        dialog.documents.request_new_visual().unwrap();
        dialog
            .documents
            .replace_visual_label("Older document label")
            .unwrap();
        dialog.documents.save_as(&path).unwrap();
        dialog.open();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);

        click_action(&mut dialog, &ctx, "visual_label");
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::Text(" discarded edit".into())],
        );
        assert_ne!(
            dialog.documents.visual_label(),
            Some("Untitled visual diagram")
        );
        assert!(dialog.documents.is_dirty());

        dialog.test_stage_document_intent(DocumentIntent::NewVisual);
        frame(&mut dialog, &ctx);
        assert_eq!(
            dialog.documents.pending_transition(),
            Some(&PendingDiagramTransition::NewVisual)
        );
        dialog.test_stage_document_intent(DocumentIntent::ResolvePending(
            TransitionDecision::Discard,
        ));
        frame(&mut dialog, &ctx);
        assert_eq!(
            dialog.documents.visual_label(),
            Some("Untitled visual diagram")
        );
        click_action(&mut dialog, &ctx, "visual_label");
        send_child_key(&mut dialog, &ctx, egui::Key::Z, egui::Modifiers::COMMAND);
        assert_eq!(
            dialog.documents.visual_label(),
            Some("Untitled visual diagram")
        );
    }

    #[test]
    fn diagram_studio_visual_export_controls_apply_scale_background_and_publish_png() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("visual.png");
        let saved_path = directory.path().join("source.mld");
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        dialog.documents.save_as(&saved_path).unwrap();
        let saved_bytes = std::fs::read(&saved_path).unwrap();
        let projection = project_visual_scene(dialog.documents.visual_scene().unwrap()).unwrap();
        let bounds = projection.bounds.expect("the title creates visible bounds");

        open_viewer(&mut dialog, &ctx);
        let viewer_before = dialog.viewer_camera;
        click_viewer_canvas(&mut dialog, &ctx);
        send_viewer_key(
            &mut dialog,
            &ctx,
            egui::Key::ArrowRight,
            egui::Modifiers::COMMAND,
        );
        click_viewer_action(&mut dialog, &ctx, "visual_viewer_zoom_in");
        let viewer_camera = dialog.viewer_camera;
        assert_ne!(viewer_camera.center, viewer_before.center);
        assert_ne!(viewer_camera.zoom, viewer_before.zoom);
        click_action(&mut dialog, &ctx, "visual_export_options");
        for _ in 0..20 {
            frame(&mut dialog, &ctx);
        }
        click_action(&mut dialog, &ctx, "visual_export_scale_two");
        assert_eq!(dialog.export_scale, ExportScale::Two);
        click_action(&mut dialog, &ctx, "visual_export_background_white");
        assert_eq!(
            dialog.visual_export_background,
            Some(VisualColor::rgba(255, 255, 255, 255))
        );

        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        click_action(&mut dialog, &ctx, "visual_export_png");
        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert!(matches!(
            dialog.visual_asset_work.as_ref().map(|work| &work.kind),
            Some(VisualAssetWorkKind::Export)
        ));
        assert!(
            !destination.exists(),
            "the picker pass only starts encoding"
        );

        dialog.poll_visual_asset_work();
        assert!(dialog.completed_visual_export.is_some());
        assert!(
            !destination.exists(),
            "the completed stage is not published yet"
        );
        frame_with_root_discard(&mut dialog, &ctx);
        assert!(dialog.completed_visual_export.is_some());
        assert!(
            !destination.exists(),
            "a discarded root pass retains the stage"
        );

        dialog.test_request_child_discard();
        frame(&mut dialog, &ctx);
        assert!(
            dialog.completed_visual_export.is_some(),
            "discarded child pass retains the stage"
        );
        assert!(!destination.exists());

        frame(&mut dialog, &ctx);
        let bytes = std::fs::read(&destination).expect("accepted visual PNG is published");
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (
                (bounds.width() * 2.0 + 48.0).ceil() as u32,
                (bounds.height() * 2.0 + 48.0).ceil() as u32,
            )
        );
        assert_eq!(decoded.get_pixel(0, 0).0, [255, 255, 255, 255]);
        assert_eq!(dialog.viewer_camera.center, viewer_camera.center);
        assert_eq!(dialog.viewer_camera.zoom, viewer_camera.zoom);
        assert_eq!(dialog.viewer_camera.fit_next, viewer_camera.fit_next);
        assert_eq!(std::fs::read(&saved_path).unwrap(), saved_bytes);
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Info(message))
                if message.contains("PNG image")
                    && message.contains("revision 0")
                    && message.contains("visual.png")
        ));
    }

    #[test]
    fn diagram_studio_visual_export_svg_and_transparent_png_use_actual_controls() {
        let directory = tempfile::tempdir().unwrap();
        let svg_path = directory.path().join("scene.svg");
        let png_path = directory.path().join("scene.png");
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let transform = VisualTransform {
            x: 300.0,
            y: 0.0,
            width: 80.0,
            height: 60.0,
            rotation_degrees: 0.0,
        };
        for color in [
            VisualColor::rgba(220, 30, 30, 255),
            VisualColor::rgba(20, 50, 230, 255),
        ] {
            dialog
                .documents
                .apply_visual_transaction(&[VisualOperation::AddObject(VisualObjectDraft {
                    layer_id: DEFAULT_LAYER_ID,
                    transform,
                    style: VisualStyle {
                        fill: color,
                        ..VisualStyle::default()
                    },
                    kind: VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                })])
                .unwrap();
        }
        let bounds = project_visual_scene(dialog.documents.visual_scene().unwrap())
            .unwrap()
            .bounds
            .unwrap();
        let expected_svg = DiagramVisualSnapshot::capture(
            dialog.documents.visual_revision().unwrap(),
            Arc::new(project_visual_scene(dialog.documents.visual_scene().unwrap()).unwrap()),
        )
        .encode_svg(DiagramVisualExportOptions::default())
        .unwrap();
        dialog.test_set_next_file_picker_result(Some(svg_path.clone()));
        click_action(&mut dialog, &ctx, "visual_export_options");
        for _ in 0..2 {
            frame(&mut dialog, &ctx);
        }
        click_action(&mut dialog, &ctx, "visual_export_svg");
        assert!(!svg_path.exists());
        frame(&mut dialog, &ctx);
        let svg_bytes = std::fs::read(&svg_path).unwrap();
        assert_eq!(
            svg_bytes,
            expected_svg.bytes(),
            "actual SVG must use the current overlapping scene"
        );
        let svg = std::str::from_utf8(&svg_bytes).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<path"));
        dialog.test_set_next_file_picker_result(Some(png_path.clone()));
        click_action(&mut dialog, &ctx, "visual_export_options");
        for _ in 0..2 {
            frame(&mut dialog, &ctx);
        }
        click_action(&mut dialog, &ctx, "visual_export_background_transparent");
        click_action(&mut dialog, &ctx, "visual_export_png");
        assert!(!png_path.exists());
        frame(&mut dialog, &ctx);
        let png = image::load_from_memory(&std::fs::read(png_path).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(png.get_pixel(0, 0).0[3], 0);
        let x = (300.0 - bounds.min.x + 24.0).floor() as u32;
        let y = (0.0 - bounds.min.y + 24.0).floor() as u32;
        let overlap = png.get_pixel(x, y).0;
        assert!(
            overlap[2] > overlap[0],
            "later blue shape must appear above red: {overlap:?}"
        );
        assert_eq!(dialog.test_file_picker_calls(), 2);
    }

    #[test]
    fn diagram_studio_visual_export_discarded_options_do_not_change_choices() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        click_action(&mut dialog, &ctx, "visual_export_options");
        for _ in 0..2 {
            frame(&mut dialog, &ctx);
        }
        assert_eq!(
            dialog.test_action_enabled("visual_export_scale_two"),
            Some(true)
        );
        click_action_with_root_discard(&mut dialog, &ctx, "visual_export_scale_two");
        assert_eq!(dialog.export_scale, ExportScale::One);
        click_action_with_child_discard(&mut dialog, &ctx, "visual_export_scale_two");
        assert_eq!(dialog.export_scale, ExportScale::One);
        click_action(&mut dialog, &ctx, "visual_export_scale_two");
        assert_eq!(dialog.export_scale, ExportScale::Two);
        assert_eq!(dialog.visual_export_background, None);
        click_action_with_root_discard(&mut dialog, &ctx, "visual_export_background_white");
        assert_eq!(dialog.visual_export_background, None);
        click_action(&mut dialog, &ctx, "visual_export_background_white");
        assert_eq!(
            dialog.visual_export_background,
            Some(VisualColor::rgba(255, 255, 255, 255))
        );
    }

    #[test]
    fn diagram_studio_visual_export_uses_same_pass_accepted_scale() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("scale.png");
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        let bounds = project_visual_scene(dialog.documents.visual_scene().unwrap())
            .unwrap()
            .bounds
            .unwrap();
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_visual_export_scale_requested = Some(ExportScale::Two);
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Png,
            ExportScale::One,
        ));
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.export_scale, ExportScale::Two);
        frame(&mut dialog, &ctx);
        let png = image::load_from_memory(&std::fs::read(destination).unwrap()).unwrap();
        assert_eq!(
            (png.width(), png.height()),
            (
                (bounds.width() * 2.0 + 48.0).ceil() as u32,
                (bounds.height() * 2.0 + 48.0).ceil() as u32
            )
        );
    }

    #[test]
    fn diagram_studio_visual_export_rejects_host_only_change_during_picker() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("host.svg");
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        let identity = dialog.render_document.clone();
        let revision = dialog.documents.visual_revision();
        let host = dialog.owner_host.identity();
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_picker_host_change = true;
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.render_document, identity);
        assert_eq!(dialog.documents.visual_revision(), revision);
        assert_ne!(dialog.owner_host.identity(), host);
        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert!(dialog.visual_asset_work.is_none());
        assert!(!destination.exists());
    }

    #[test]
    fn diagram_studio_visual_export_cancel_dominates_same_pass_save() {
        let directory = tempfile::tempdir().unwrap();
        let native = directory.path().join("source.mld");
        let destination = directory.path().join("cancel.svg");
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        dialog.documents.save_as(&native).unwrap();
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        frame(&mut dialog, &ctx);
        let mut work = dialog.visual_asset_work.take().expect("real export worker");
        let queued = work.receiver.try_recv().expect("test worker staged result");
        let temporary = match &queued {
            VisualAssetWorkResult::Export(Ok(export)) => {
                export.staged.temporary_path_for_test().to_path_buf()
            }
            _ => panic!("expected a staged export"),
        };
        let (sender, receiver) = sync_channel(1);
        work.receiver = receiver;
        dialog.visual_asset_work = Some(work);
        frame(&mut dialog, &ctx);
        assert_eq!(
            dialog.test_action_enabled("visual_export_cancel"),
            Some(true)
        );
        let pos = action_center(&dialog, "visual_export_cancel");
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(pos), button(true)],
        );
        assert!(
            dialog.visual_asset_work.is_some() && !destination.exists(),
            "actual Cancel press must precede the queued completion"
        );
        assert!(sender.send(queued).is_ok());
        drop(sender);
        dialog.test_stage_document_intent(DocumentIntent::Save);
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(pos), button(false)],
        );
        assert!(dialog.completed_visual_export.is_none());
        assert!(
            !destination.exists(),
            "cancel release published: message={:?}, action={:?}",
            dialog.message,
            dialog.test_action_rect("visual_export_cancel")
        );
        assert!(!temporary.exists());
        assert!(native.exists());
    }

    #[test]
    fn diagram_studio_visual_export_stage_blocks_asset_import_and_hydration() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("held.svg");
        let image_path = directory.path().join("asset.png");
        write_test_png(&image_path, 4, 4);
        let prepared = crate::diagram::prepare_visual_asset_file(&image_path, usize::MAX).unwrap();
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        frame(&mut dialog, &ctx);
        dialog.poll_visual_asset_work();
        assert!(dialog.completed_visual_export.is_some());
        let mut scene = dialog.documents.visual_scene().unwrap().clone();
        scene.assets.push(prepared.asset);
        let revision = dialog.documents.visual_revision().unwrap();
        dialog.start_visual_asset_hydration(&ctx, &scene, revision);
        assert!(dialog.visual_asset_work.is_none());
        let fence = VisualAssetImportFence {
            document: dialog.render_document.clone(),
            revision,
            owner_generation: dialog.owner_generation,
            owner_viewport_id: viewport_id(),
            start_pass: ViewportPassProvenance::current(&ctx, viewport_id()),
            start_pass_accepted: true,
        };
        dialog.start_visual_asset_import(&ctx, image_path, fence);
        assert!(dialog.visual_asset_work.is_none());
        assert!(dialog.completed_visual_export.is_some());
        assert!(!destination.exists());
    }

    #[test]
    fn diagram_studio_visual_export_same_pass_edit_uses_accepted_revision() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("accepted.svg");
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        let original_revision = dialog.documents.visual_revision().unwrap();
        let expected_document = dialog.render_document.clone();
        let label = "Accepted in the export pass";
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_pending_mutation = Some(PendingMutation::ReplaceVisualLabel {
            label: label.into(),
            expected_document,
            expected_revision: original_revision,
        });
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));

        frame(&mut dialog, &ctx);
        let accepted_revision = original_revision + 1;
        assert_eq!(dialog.documents.visual_revision(), Some(accepted_revision));
        assert_eq!(dialog.documents.visual_label(), Some(label));
        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert!(!destination.exists());

        let expected_projection =
            Arc::new(project_visual_scene(dialog.documents.visual_scene().unwrap()).unwrap());
        let expected = DiagramVisualSnapshot::capture(accepted_revision, expected_projection)
            .encode_svg(DiagramVisualExportOptions::default())
            .unwrap();
        frame(&mut dialog, &ctx);
        assert_eq!(std::fs::read(&destination).unwrap(), expected.bytes());
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Info(message))
                if message.contains(&format!("revision {accepted_revision}"))
        ));
    }

    #[test]
    fn diagram_studio_visual_export_discarded_click_does_not_open_picker() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("discarded.svg");
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        dialog.test_set_next_file_picker_result(Some(destination.clone()));

        click_action(&mut dialog, &ctx, "visual_export_options");
        for _ in 0..2 {
            frame(&mut dialog, &ctx);
        }
        click_action_with_root_discard(&mut dialog, &ctx, "visual_export_svg");

        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert!(!destination.exists());
        assert!(dialog.visual_asset_work.is_none());
        assert!(dialog.completed_visual_export.is_none());
    }

    #[test]
    fn diagram_studio_visual_export_rechecks_revision_after_picker() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("stale.svg");
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        let initial_revision = dialog.documents.visual_revision().unwrap();
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_set_picker_visual_change("Changed inside the picker hook");

        click_action(&mut dialog, &ctx, "visual_export_options");
        for _ in 0..2 {
            frame(&mut dialog, &ctx);
        }
        click_action(&mut dialog, &ctx, "visual_export_svg");

        assert_eq!(
            dialog.documents.visual_revision(),
            Some(initial_revision + 1)
        );
        assert_eq!(
            dialog.documents.visual_label(),
            Some("Changed inside the picker hook")
        );
        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert!(!destination.exists());
        assert!(dialog.visual_asset_work.is_none());
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Error(message)) if message.contains("changed while choosing")
        ));
    }

    #[test]
    fn diagram_studio_visual_export_worker_result_is_retired_after_document_identity_changes() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("retired.png");
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Png,
            ExportScale::One,
        ));
        frame(&mut dialog, &ctx);
        assert!(dialog.visual_asset_work.is_some());
        let temporary = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".diagram-copy-")
            })
            .expect("worker result owns a staged temporary before collection");
        assert!(
            dialog.completed_visual_export.is_none(),
            "worker result remains queued"
        );
        let old_identity = dialog.render_document.clone();

        assert_eq!(
            dialog.documents.request_new_visual().unwrap(),
            TransitionOutcome::Applied
        );
        dialog.reset_document_presentation();
        dialog.poll_visual_asset_work();

        assert_ne!(dialog.render_document, old_identity);
        assert!(dialog.completed_visual_export.is_none());
        assert!(dialog.visual_asset_work.is_none());
        assert!(!destination.exists());
        assert!(!temporary.exists());
    }

    #[test]
    fn diagram_studio_visual_export_staged_result_is_dropped_on_accepted_close() {
        let directory = tempfile::tempdir().unwrap();
        let saved_path = directory.path().join("clean-before-close.mld");
        let destination = directory.path().join("closed.svg");
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        dialog.documents.save_as(&saved_path).unwrap();
        let saved_bytes = std::fs::read(&saved_path).unwrap();
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        frame(&mut dialog, &ctx);
        dialog.poll_visual_asset_work();
        let temporary = dialog
            .completed_visual_export
            .as_ref()
            .expect("test worker returns a stage before the close")
            .staged
            .temporary_path_for_test()
            .to_path_buf();
        assert!(temporary.exists());
        assert!(!destination.exists());

        dialog.request_close();
        frame(&mut dialog, &ctx);

        assert!(!dialog.open, "the clean document close is accepted");
        assert!(dialog.completed_visual_export.is_none());
        assert!(dialog.visual_asset_work.is_none());
        assert!(!destination.exists());
        assert!(!temporary.exists());
        assert_eq!(std::fs::read(&saved_path).unwrap(), saved_bytes);
    }

    #[test]
    fn diagram_studio_visual_export_late_collision_preserves_destination_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("occupied.svg");
        let occupied = b"preserve a file created after staging";
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_stage_document_intent(DocumentIntent::Export(
            DiagramExportFormat::Svg,
            ExportScale::One,
        ));
        frame(&mut dialog, &ctx);
        dialog.poll_visual_asset_work();
        let temporary = dialog
            .completed_visual_export
            .as_ref()
            .expect("test worker returns a stage before publication")
            .staged
            .temporary_path_for_test()
            .to_path_buf();
        std::fs::write(&destination, occupied).unwrap();

        frame(&mut dialog, &ctx);

        assert_eq!(std::fs::read(&destination).unwrap(), occupied);
        assert!(!temporary.exists());
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Error(message)) if message.contains("occupied")
        ));
    }

    #[test]
    fn diagram_studio_visual_export_cancel_retires_only_export_work_until_worker_settles() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        let host_only_hidden_demand = dialog.needs_root_ui_when_hidden();
        let document = dialog.render_document.clone();
        let revision = dialog.documents.visual_revision().unwrap();
        let owner_generation = dialog.owner_generation;
        let owner_viewport_id = viewport_id();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = sync_channel(1);
        dialog.visual_asset_work = Some(PendingVisualAssetWork {
            document: document.clone(),
            revision,
            owner_generation,
            owner_viewport_id,
            kind: VisualAssetWorkKind::Export,
            cancelled: cancelled.clone(),
            receiver,
            hydration_ids: Vec::new(),
            retired: false,
        });
        frame(&mut dialog, &ctx);
        assert_eq!(
            dialog.test_action_enabled("visual_export_cancel"),
            Some(true)
        );

        click_action(&mut dialog, &ctx, "visual_export_cancel");

        assert!(cancelled.load(Ordering::Acquire));
        assert!(
            dialog
                .visual_asset_work
                .as_ref()
                .is_some_and(|work| work.retired)
        );
        assert!(dialog.needs_root_ui_when_hidden());
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Info(message)) if message.contains("slot remains occupied")
        ));
        drop(sender);
        frame(&mut dialog, &ctx);
        assert!(dialog.visual_asset_work.is_none());
        assert!(dialog.completed_visual_export.is_none());
        assert_eq!(dialog.needs_root_ui_when_hidden(), host_only_hidden_demand);

        let mut asset_dialog = initialized_visual_dialog();
        let asset_ctx = egui::Context::default();
        frame(&mut asset_dialog, &asset_ctx);
        let asset_document = asset_dialog.render_document.clone();
        let asset_revision = asset_dialog.documents.visual_revision().unwrap();
        let asset_generation = asset_dialog.owner_generation;
        let (asset_sender, asset_receiver) = sync_channel(1);
        asset_dialog.visual_asset_work = Some(PendingVisualAssetWork {
            document: asset_document.clone(),
            revision: asset_revision,
            owner_generation: asset_generation,
            owner_viewport_id: viewport_id(),
            kind: VisualAssetWorkKind::Import(VisualAssetImportFence {
                document: asset_document,
                revision: asset_revision,
                owner_generation: asset_generation,
                owner_viewport_id: viewport_id(),
                start_pass: ViewportPassProvenance::current(&asset_ctx, viewport_id()),
                start_pass_accepted: true,
            }),
            cancelled: Arc::new(AtomicBool::new(false)),
            receiver: asset_receiver,
            hydration_ids: Vec::new(),
            retired: false,
        });
        frame(&mut asset_dialog, &asset_ctx);
        assert_eq!(
            asset_dialog.test_action_enabled("visual_export_cancel"),
            None
        );
        assert!(!asset_dialog.visual_asset_work.as_ref().unwrap().retired);
        drop(asset_sender);
        frame(&mut asset_dialog, &asset_ctx);
    }

    #[test]
    fn diagram_studio_visual_scene_accepted_property_edit_is_one_undo_step_and_cache_is_shared() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        assert_eq!(dialog.test_visual_projection_build_count, 1);
        assert_eq!(dialog.test_visual_items_omitted, 0);

        let mut title = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
            .unwrap()
            .clone();
        title.transform.x = 48.0;
        let document = dialog.render_document.clone();
        let revision = dialog.documents.visual_revision().unwrap();
        dialog.visual_selection = Some(crate::diagram::TITLE_OBJECT_ID);
        dialog.visual_property_draft = Some(VisualPropertyDraft {
            document,
            basis_revision: revision,
            object: title,
        });

        click_action(&mut dialog, &ctx, "visual_apply_properties");
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 1));
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .objects
                .iter()
                .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
                .unwrap()
                .transform
                .x,
            48.0
        );
        assert!(dialog.documents.can_undo());
        assert!(dialog.visual_property_draft.is_none());

        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_visual_projection_build_count, 2);
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_visual_projection_build_count, 2);
        assert_eq!(
            dialog.test_visual_items_painted,
            dialog
                .visual_projection_cache
                .as_ref()
                .unwrap()
                .projection
                .as_ref()
                .unwrap()
                .items
                .len()
        );

        open_viewer(&mut dialog, &ctx);
        assert_eq!(dialog.test_visual_projection_build_count, 2);
        assert_eq!(dialog.test_viewer_visual_projection_items, 1);
        assert_eq!(dialog.test_viewer_visual_items_painted, 1);
        assert!(dialog.viewer_open);

        click_action(&mut dialog, &ctx, "visual_undo");
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 2));
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .objects
                .iter()
                .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
                .unwrap()
                .transform
                .x,
            0.0
        );
        assert!(dialog.documents.can_redo());
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_visual_projection_build_count, 3);
    }

    #[test]
    fn diagram_studio_visual_connector_controls_create_edit_select_and_delete() {
        let mut dialog = initialized_visual_dialog();
        let endpoints = dialog
            .documents
            .apply_visual_transaction(&[
                VisualOperation::AddObject(VisualObjectDraft {
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: -180.0,
                        y: 260.0,
                        width: 100.0,
                        height: 60.0,
                        rotation_degrees: 0.0,
                    },
                    style: VisualStyle::default(),
                    kind: VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                }),
                VisualOperation::AddObject(VisualObjectDraft {
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: 180.0,
                        y: 260.0,
                        width: 100.0,
                        height: 60.0,
                        rotation_degrees: 0.0,
                    },
                    style: VisualStyle::default(),
                    kind: VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Diamond,
                    },
                }),
            ])
            .unwrap()
            .created_objects;
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        let before_create = dialog.documents.visual_revision().unwrap();
        click_action(&mut dialog, &ctx, "visual_add_connector");
        assert_eq!(dialog.documents.visual_revision(), Some(before_create + 1));
        let connector_id = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find_map(|object| {
                matches!(&object.kind, VisualObjectKind::Connector { .. }).then_some(object.id)
            })
            .expect("Add connector creates a persisted connector object");

        click_action(&mut dialog, &ctx, "visual_object_selection");
        click_action(
            &mut dialog,
            &ctx,
            &format!("visual_object_option_{}", connector_id.get()),
        );
        assert_eq!(dialog.visual_selection, Some(connector_id));
        frame(&mut dialog, &ctx);
        assert!(dialog.test_action_rect("visual_connector_from").is_some());
        assert!(dialog.test_action_rect("visual_connector_route").is_some());
        assert!(
            dialog
                .test_action_rect("visual_connector_direction")
                .is_some()
        );
        assert!(dialog.test_action_rect("visual_connector_label").is_some());

        click_action(&mut dialog, &ctx, "visual_connector_from");
        click_action(
            &mut dialog,
            &ctx,
            &format!("visual_connector_from_object_{}", endpoints[1].get()),
        );
        click_action(&mut dialog, &ctx, "visual_connector_route");
        click_action(&mut dialog, &ctx, "visual_connector_route_option_curved");
        click_action(&mut dialog, &ctx, "visual_connector_direction");
        click_action(
            &mut dialog,
            &ctx,
            "visual_connector_direction_option_both_ends",
        );
        reveal_visual_action(&mut dialog, &ctx, "visual_connector_label");
        click_action(&mut dialog, &ctx, "visual_connector_label");
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::Text("review path".into())],
        );
        let staged_edit = dialog
            .visual_property_draft
            .as_ref()
            .expect("connector edits remain a draft until Apply")
            .object
            .clone();
        let revision_before_discard = dialog.documents.visual_revision().unwrap();
        reveal_visual_action(&mut dialog, &ctx, "visual_apply_properties");
        click_action_with_child_discard(&mut dialog, &ctx, "visual_apply_properties");
        assert_eq!(
            dialog.documents.visual_revision(),
            Some(revision_before_discard)
        );
        assert_eq!(
            dialog
                .visual_property_draft
                .as_ref()
                .map(|draft| draft.object.clone()),
            Some(staged_edit.clone())
        );
        assert!(matches!(
            &dialog
                .documents
                .visual_scene()
                .unwrap()
                .objects
                .iter()
                .find(|object| object.id == connector_id)
                .unwrap()
                .kind,
            VisualObjectKind::Connector { connector }
                if connector.route == VisualConnectorRoute::Elbow
                    && connector.direction == VisualConnectorDirection::Forward
                    && connector.label.is_empty()
        ));

        click_action(&mut dialog, &ctx, "visual_apply_properties");
        assert_eq!(
            dialog.documents.visual_revision(),
            Some(revision_before_discard + 1)
        );
        let connector = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == connector_id)
            .unwrap();
        assert!(matches!(
            &connector.kind,
            VisualObjectKind::Connector { connector }
                if connector.from.object_id == endpoints[1]
                    && connector.route == VisualConnectorRoute::Curved
                    && connector.direction == VisualConnectorDirection::Both
                    && connector.label.contains("review path")
        ));
        assert!(dialog.documents.can_undo());

        frame(&mut dialog, &ctx);
        reveal_visual_action(&mut dialog, &ctx, "open_viewer");
        let projection_builds = dialog.test_visual_projection_build_count;
        open_viewer(&mut dialog, &ctx);
        assert_eq!(dialog.test_visual_projection_build_count, projection_builds);
        assert_eq!(dialog.test_viewer_visual_projection_items, 4);
        let before_remove = dialog.documents.visual_revision().unwrap();
        reveal_visual_action(&mut dialog, &ctx, "visual_remove_object");
        click_action(&mut dialog, &ctx, "visual_remove_object");
        assert_eq!(dialog.documents.visual_revision(), Some(before_remove + 1));
        assert!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .objects
                .iter()
                .all(|object| object.id != connector_id)
        );
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.test_viewer_visual_projection_items, 3);
    }

    #[test]
    fn diagram_studio_visual_connector_endpoint_drag_previews_and_commits() {
        let mut dialog = initialized_visual_dialog();
        let endpoints = dialog
            .documents
            .apply_visual_transaction(&[
                VisualOperation::AddObject(VisualObjectDraft {
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: -180.0,
                        y: 300.0,
                        width: 100.0,
                        height: 60.0,
                        ..VisualTransform::default()
                    },
                    style: VisualStyle::default(),
                    kind: VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                }),
                VisualOperation::AddObject(VisualObjectDraft {
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: 180.0,
                        y: 300.0,
                        width: 100.0,
                        height: 60.0,
                        ..VisualTransform::default()
                    },
                    style: VisualStyle::default(),
                    kind: VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Ellipse,
                    },
                }),
            ])
            .unwrap()
            .created_objects;
        let connector_id = dialog
            .documents
            .apply_visual_transaction(&[VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform::default(),
                style: VisualStyle::default(),
                kind: VisualObjectKind::Connector {
                    connector: VisualConnector {
                        from: VisualConnectorEndpoint {
                            object_id: endpoints[0],
                            anchor: VisualAnchor::East,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: endpoints[1],
                            anchor: VisualAnchor::West,
                        },
                        route: VisualConnectorRoute::Straight,
                        direction: VisualConnectorDirection::None,
                        label: String::new(),
                    },
                },
            })])
            .unwrap()
            .created_objects[0];
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let original_scene = dialog.documents.visual_scene().unwrap().clone();
        let revision = dialog.documents.visual_revision().unwrap();
        let committed_route = project_visual_scene(&original_scene)
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.object_id == connector_id)
            .unwrap()
            .open_paths[0]
            .clone();
        let start = visual_canvas_world_point(
            &dialog,
            ScenePoint {
                x: -180.0,
                y: 300.0,
            },
        );
        let end = visual_canvas_world_point(
            &dialog,
            ScenePoint {
                x: -220.0,
                y: 260.0,
            },
        );
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(start), button(start, true)],
        );
        frame_with_child_events(&mut dialog, &ctx, vec![egui::Event::PointerMoved(end)]);
        let active = dialog
            .visual_gesture
            .as_ref()
            .expect("dragging an endpoint object retains the gesture preview");
        let preview_route = active
            .preview_projection
            .as_ref()
            .and_then(|projection| {
                projection
                    .items
                    .iter()
                    .find(|item| item.object_id == connector_id)
            })
            .expect("preview projection reroutes the connected edge")
            .open_paths[0]
            .clone();
        assert_ne!(preview_route, committed_route);
        assert_eq!(dialog.documents.visual_revision(), Some(revision));
        assert_eq!(dialog.documents.visual_scene().unwrap(), &original_scene);

        frame_with_child_events(
            &mut dialog,
            &ctx,
            vec![egui::Event::PointerMoved(end), button(end, false)],
        );
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 1));
        assert!(dialog.visual_gesture.is_none());
        let committed_scene = dialog.documents.visual_scene().unwrap();
        let moved_source = committed_scene
            .objects
            .iter()
            .find(|object| object.id == endpoints[0])
            .unwrap();
        assert_ne!(moved_source.transform, original_scene.objects[1].transform);
        let committed_after_release = project_visual_scene(committed_scene)
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.object_id == connector_id)
            .unwrap()
            .open_paths[0]
            .clone();
        assert_eq!(committed_after_release, preview_route);
    }

    #[test]
    fn diagram_studio_visual_scene_discarded_and_rejected_properties_keep_the_draft() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let original = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
            .unwrap()
            .clone();
        let mut edited = original.clone();
        edited.transform.x = 77.0;
        let identity = dialog.render_document.clone();
        let revision = dialog.documents.visual_revision().unwrap();
        dialog.visual_selection = Some(edited.id);
        dialog.visual_property_draft = Some(VisualPropertyDraft {
            document: identity.clone(),
            basis_revision: revision,
            object: edited.clone(),
        });
        click_action_with_child_discard(&mut dialog, &ctx, "visual_apply_properties");
        assert_eq!(dialog.documents.visual_revision(), Some(revision));
        assert_eq!(
            dialog.documents.visual_scene().unwrap().objects[0],
            original
        );
        assert_eq!(
            dialog
                .visual_property_draft
                .as_ref()
                .map(|draft| draft.object.clone()),
            Some(edited.clone())
        );

        let mut invalid = edited.clone();
        invalid.transform.width = 0.0;
        dialog.visual_property_draft = Some(VisualPropertyDraft {
            document: identity,
            basis_revision: revision,
            object: invalid.clone(),
        });
        click_action(&mut dialog, &ctx, "visual_apply_properties");
        assert_eq!(dialog.documents.visual_revision(), Some(revision));
        assert_eq!(
            dialog.documents.visual_scene().unwrap().objects[0]
                .transform
                .x,
            0.0
        );
        assert_eq!(
            dialog
                .visual_property_draft
                .as_ref()
                .map(|draft| draft.object.clone()),
            Some(invalid)
        );
    }

    #[test]
    fn diagram_studio_visual_scene_stale_identity_and_retired_owner_effects_are_rejected() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let old_document = dialog.render_document.clone();
        let old_revision = dialog.documents.visual_revision().unwrap();
        assert_eq!(old_revision, 0);

        assert_eq!(
            dialog.documents.request_new_visual().unwrap(),
            TransitionOutcome::Applied
        );
        dialog.reset_document_presentation();
        assert_eq!(dialog.documents.visual_revision(), Some(old_revision));
        assert_ne!(dialog.render_document, old_document);
        dialog.test_pending_mutation = Some(PendingMutation::VisualTransaction {
            operations: vec![VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform::default(),
                style: VisualStyle::default(),
                kind: VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Diamond,
                },
            })],
            expected_document: old_document,
            expected_revision: old_revision,
            select_after: None,
        });
        frame(&mut dialog, &ctx);
        assert_eq!(dialog.documents.visual_revision(), Some(old_revision));
        assert_eq!(dialog.documents.visual_scene().unwrap().objects.len(), 1);

        let current_document = dialog.render_document.clone();
        dialog.test_pending_mutation = Some(PendingMutation::VisualTransaction {
            operations: vec![VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform::default(),
                style: VisualStyle::default(),
                kind: VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Triangle,
                },
            })],
            expected_document: current_document,
            expected_revision: old_revision,
            select_after: None,
        });
        frame_with_retired_owner_generation(&mut dialog, &ctx);
        assert_eq!(dialog.documents.visual_revision(), Some(old_revision));
        assert_eq!(dialog.documents.visual_scene().unwrap().objects.len(), 1);
    }

    #[test]
    fn diagram_studio_visual_scene_paint_work_is_bounded_and_reports_omissions() {
        let mut dialog = initialized_visual_dialog();
        for batch in 0..8 {
            let operations = (0..64)
                .map(|offset| {
                    let index = batch * 64 + offset;
                    VisualOperation::AddObject(VisualObjectDraft {
                        layer_id: DEFAULT_LAYER_ID,
                        transform: VisualTransform {
                            x: f64::from(index % 32) * 30.0,
                            y: f64::from(index / 32) * 30.0,
                            width: 25.0,
                            height: 18.0,
                            rotation_degrees: 0.0,
                        },
                        style: VisualStyle::default(),
                        kind: VisualObjectKind::Primitive {
                            shape: VisualPrimitiveShape::Ellipse,
                        },
                    })
                })
                .collect::<Vec<_>>();
            dialog
                .documents
                .apply_visual_transaction(&operations)
                .unwrap();
        }
        dialog.open();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        let projected_items = dialog
            .visual_projection_cache
            .as_ref()
            .unwrap()
            .projection
            .as_ref()
            .unwrap()
            .items
            .len();
        assert_eq!(projected_items, 513);
        assert!(dialog.test_visual_items_omitted > 0);
        assert_eq!(
            dialog.test_visual_items_painted + dialog.test_visual_items_omitted,
            projected_items
        );
    }

    #[test]
    fn diagram_drawio_workflow_import_reports_unsupported_frame_without_candidate() {
        let directory = tempfile::tempdir().unwrap();
        let source_path = directory.path().join("unsupported.drawio");
        let source = b"<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"layer\" parent=\"0\"/><mxCell id=\"bad-lane\" parent=\"layer\" vertex=\"1\" style=\"swimlane;startSize=40;\"><mxGeometry x=\"0\" y=\"0\" width=\"100\" height=\"100\" as=\"geometry\"/></mxCell></root></mxGraphModel>";
        std::fs::write(&source_path, source).unwrap();
        let mut dialog = initialized_dialog();
        let before = dialog.test_session().draft().to_owned();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        dialog.test_set_next_file_picker_result(Some(source_path.clone()));
        click_mermaid_drawio_import(&mut dialog, &ctx);
        wait_for_drawio_review(&mut dialog, &ctx);
        assert!(
            matches!(dialog.drawio_review.as_ref(), Some(DrawioWorkflowReview::ImportUnsupported { report, .. }) if report.issues.iter().any(|issue| issue.source_id.as_deref() == Some("bad-lane") && issue.property == "swimlane appearance is not representable"))
        );
        assert_eq!(dialog.test_session().draft(), before);
        assert_eq!(std::fs::read(&source_path).unwrap(), source);
    }

    #[test]
    fn diagram_drawio_workflow_import_copy_then_open_uses_dirty_transition() {
        let directory = tempfile::tempdir().expect("temporary draw.io workflow directory");
        let active_path = directory.path().join("working.mld");
        let source_path = directory.path().join("incoming.drawio");
        let occupied_copy = directory.path().join("occupied.mld");
        let copy_path = directory.path().join("incoming-imported.mld");
        std::fs::write(&source_path, DRAWIO_WORKFLOW_FIXTURE.as_bytes())
            .expect("write bounded draw.io source");
        let source_bytes = std::fs::read(&source_path).expect("snapshot draw.io source bytes");
        let occupied_bytes = b"preserve the late collision";

        let mut dialog = initialized_dialog();
        dialog
            .documents
            .save_as(&active_path)
            .expect("establish active native baseline");
        let saved_active_bytes = std::fs::read(&active_path).expect("read active baseline");
        dialog.test_replace_source("flowchart LR\n    A[\"Unsaved working draft\"] --> B\n");
        let active_source = dialog.test_session().draft().to_owned();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        dialog.test_set_next_file_picker_result(Some(source_path.clone()));
        click_mermaid_drawio_import(&mut dialog, &ctx);
        assert_eq!(dialog.test_file_picker_calls(), 1);
        wait_for_drawio_review(&mut dialog, &ctx);
        assert!(matches!(
            dialog.drawio_review.as_ref(),
            Some(DrawioWorkflowReview::Import { source_path: reviewed, copy_path: None, .. })
                if reviewed == &source_path
        ));
        assert_eq!(dialog.documents.path(), Some(active_path.as_path()));
        assert_eq!(dialog.test_session().draft(), active_source);
        assert!(dialog.documents.is_dirty());

        std::fs::write(&occupied_copy, occupied_bytes).expect("create target after review");
        dialog.test_set_next_file_picker_result(Some(occupied_copy.clone()));
        click_action(&mut dialog, &ctx, "drawio_publish_import");
        assert_eq!(std::fs::read(&occupied_copy).unwrap(), occupied_bytes);
        assert!(matches!(
            dialog.drawio_review.as_ref(),
            Some(DrawioWorkflowReview::Import {
                copy_path: None,
                ..
            })
        ));

        dialog.test_set_next_file_picker_result(Some(copy_path.clone()));
        click_action(&mut dialog, &ctx, "drawio_publish_import");
        assert!(copy_path.is_file());
        assert_eq!(std::fs::read(&active_path).unwrap(), saved_active_bytes);
        assert_eq!(dialog.documents.path(), Some(active_path.as_path()));
        assert_eq!(dialog.test_session().draft(), active_source);
        assert!(dialog.documents.is_dirty());
        assert_eq!(std::fs::read(&source_path).unwrap(), source_bytes);
        assert!(matches!(
            dialog.drawio_review.as_ref(),
            Some(DrawioWorkflowReview::Import { copy_path: Some(path), .. }) if path == &copy_path
        ));

        click_action(&mut dialog, &ctx, "drawio_open_copy");
        assert!(dialog.documents.pending_transition().is_some());
        click_action(&mut dialog, &ctx, "decision_cancel");
        assert!(dialog.documents.pending_transition().is_none());
        assert_eq!(dialog.documents.path(), Some(active_path.as_path()));
        assert_eq!(dialog.test_session().draft(), active_source);
        assert!(dialog.documents.is_dirty());

        click_action(&mut dialog, &ctx, "drawio_open_copy");
        assert!(dialog.documents.pending_transition().is_some());
        click_action(&mut dialog, &ctx, "decision_discard");
        assert_eq!(dialog.documents.path(), Some(copy_path.as_path()));
        assert_eq!(
            dialog.documents.authoring_kind(),
            Some(crate::diagram::DiagramAuthoringKind::Visual)
        );
        assert!(dialog.documents.session().is_none());
        let scene = dialog.documents.visual_scene().expect("opened visual copy");
        let frame = scene
            .objects
            .iter()
            .find(|object| matches!(object.kind, VisualObjectKind::Container { .. }))
            .expect("imported group")
            .clone();
        assert!(
            scene
                .parent_by_child
                .values()
                .any(|parent| *parent == frame.id)
        );
        let mut edited_frame = frame.clone();
        edited_frame.transform.x += 10.0;
        dialog
            .documents
            .apply_visual_transaction(&[VisualOperation::ReplaceObject(edited_frame.clone())])
            .unwrap();
        assert!(dialog.documents.undo().unwrap());
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .objects
                .iter()
                .find(|object| object.id == frame.id),
            Some(&frame)
        );
        assert!(dialog.documents.redo().unwrap());
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .objects
                .iter()
                .find(|object| object.id == frame.id),
            Some(&edited_frame)
        );
        assert_eq!(std::fs::read(&active_path).unwrap(), saved_active_bytes);
        assert_eq!(std::fs::read(&source_path).unwrap(), source_bytes);
    }

    #[test]
    fn diagram_drawio_workflow_discard_stale_and_cancel_do_not_publish() {
        let directory = tempfile::tempdir().expect("temporary draw.io workflow directory");
        let source_path = directory.path().join("incoming.drawio");
        let never_published = directory.path().join("never-created.mld");
        std::fs::write(&source_path, DRAWIO_WORKFLOW_FIXTURE.as_bytes())
            .expect("write bounded draw.io source");
        let mut dialog = initialized_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let original_source = dialog.test_session().draft().to_owned();

        dialog.test_set_next_file_picker_result(Some(source_path.clone()));
        click_action(&mut dialog, &ctx, "menu_import");
        click_action_with_child_discard(&mut dialog, &ctx, "drawio_import");
        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert!(dialog.drawio_work.is_none());
        assert_eq!(dialog.test_session().draft(), original_source);
        assert!(!never_published.exists());

        dialog.test_set_next_file_picker_result(Some(source_path.clone()));
        dialog.test_set_picker_source_change(
            "flowchart LR\n    A[\"Changed during picker\"] --> B\n",
        );
        click_mermaid_drawio_import(&mut dialog, &ctx);
        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert!(dialog.drawio_work.is_none());
        assert!(dialog.drawio_review.is_none());
        assert_ne!(dialog.test_session().draft(), original_source);
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Error(message)) if message.contains("while choosing")
        ));
        assert!(!never_published.exists());

        dialog.test_set_next_file_picker_result(Some(source_path));
        click_mermaid_drawio_import(&mut dialog, &ctx);
        wait_for_drawio_review(&mut dialog, &ctx);
        let source_after_import = dialog.test_session().draft().to_owned();
        click_action(&mut dialog, &ctx, "drawio_cancel");
        assert!(dialog.drawio_review.is_none());
        assert!(dialog.drawio_work.is_none());
        assert_eq!(dialog.test_session().draft(), source_after_import);
        assert!(!never_published.exists());

        let candidate = import_drawio_xml(DRAWIO_WORKFLOW_FIXTURE.as_bytes()).unwrap();
        let scope = DrawioWorkflowScope {
            document: dialog.render_document.clone(),
            revision: dialog.test_session().revision(),
            owner_generation: dialog.owner_generation,
            owner_viewport_id: viewport_id(),
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = sync_channel(1);
        dialog.drawio_work = Some(PendingDrawioWork {
            scope,
            kind: DrawioWorkKind::Import {
                source_path: directory.path().join("late.drawio"),
            },
            cancelled: cancelled.clone(),
            receiver,
            retired: false,
        });
        click_action(&mut dialog, &ctx, "drawio_cancel");
        assert!(cancelled.load(Ordering::Acquire));
        assert!(dialog.drawio_work.as_ref().is_some_and(|work| work.retired));
        sender
            .send(DrawioWorkResult::Import(Ok(candidate)))
            .expect("late result is drained from the retired slot");
        drop(sender);
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        assert!(dialog.drawio_work.is_none());
        assert!(dialog.drawio_review.is_none());
        assert!(!never_published.exists());
    }

    #[test]
    fn diagram_drawio_workflow_export_review_is_atomic_and_preserves_native_document() {
        let directory = tempfile::tempdir().expect("temporary draw.io export directory");
        let native_path = directory.path().join("visual.mld");
        let occupied_path = directory.path().join("occupied.drawio");
        let output_path = directory.path().join("published.drawio");
        let occupied_bytes = b"keep existing draw.io bytes";
        std::fs::write(&occupied_path, occupied_bytes).expect("prepare occupied export target");
        let mut dialog = initialized_visual_dialog();
        dialog
            .documents
            .replace_visual_label("Committed workflow scene")
            .expect("set export scene title");
        let group_id = dialog
            .documents
            .apply_visual_transaction(&[VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform {
                    x: 40.0,
                    y: 20.0,
                    width: 240.0,
                    height: 160.0,
                    rotation_degrees: 0.0,
                },
                style: VisualStyle::default(),
                kind: VisualObjectKind::Container {
                    label: "Group frame".into(),
                },
            })])
            .unwrap()
            .created_objects[0];
        let member = dialog
            .documents
            .apply_visual_transaction(&[VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform {
                    x: 40.0,
                    y: 20.0,
                    width: 120.0,
                    height: 64.0,
                    rotation_degrees: 12.0,
                },
                style: VisualStyle::default(),
                kind: VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Rectangle,
                },
            })])
            .expect("add a supported visual object")
            .created_objects[0];
        dialog
            .documents
            .apply_visual_transaction(&[VisualOperation::SetParent {
                child: member,
                parent: Some(group_id),
            }])
            .unwrap();
        dialog
            .documents
            .save_as(&native_path)
            .expect("save the active native visual document");
        let native_bytes = std::fs::read(&native_path).expect("snapshot native bytes");
        let committed_scene = dialog.documents.visual_scene().unwrap().clone();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        click_visual_drawio_export(&mut dialog, &ctx);
        wait_for_drawio_review(&mut dialog, &ctx);
        assert!(matches!(
            dialog.drawio_review.as_ref(),
            Some(DrawioWorkflowReview::ExportReady { .. })
        ));
        std::fs::write(&occupied_path, occupied_bytes).expect("create collision after review");
        dialog.test_set_next_file_picker_result(Some(occupied_path.clone()));
        click_action(&mut dialog, &ctx, "drawio_publish_export");
        assert_eq!(std::fs::read(&occupied_path).unwrap(), occupied_bytes);
        assert!(matches!(
            dialog.drawio_review.as_ref(),
            Some(DrawioWorkflowReview::ExportReady { .. })
        ));

        dialog.test_set_next_file_picker_result(Some(output_path.clone()));
        click_action(&mut dialog, &ctx, "drawio_publish_export");
        let bytes = std::fs::read(&output_path).expect("accepted export is published");
        let imported = import_drawio_xml(&bytes).expect("published XML parses in the wire adapter");
        assert!(imported.document.scene().objects.iter().any(|object| {
            matches!(
                &object.kind,
                VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Rectangle
                }
            )
        }));
        assert_eq!(imported.document.scene().parent_by_child.len(), 1);
        let original_frame = committed_scene
            .objects
            .iter()
            .find(|object| object.id == group_id)
            .unwrap();
        let imported_frame = imported
            .document
            .scene()
            .objects
            .iter()
            .find(|object| {
                matches!(&object.kind, VisualObjectKind::Container { label } if label == "Group frame")
            })
            .expect("visible container kind and label survive export");
        assert_eq!(imported_frame.transform, original_frame.transform);
        assert_eq!(imported_frame.style, original_frame.style);
        assert_eq!(dialog.documents.visual_scene().unwrap(), &committed_scene);
        assert_eq!(dialog.documents.path(), Some(native_path.as_path()));
        assert_eq!(std::fs::read(&native_path).unwrap(), native_bytes);
        assert!(dialog.drawio_review.is_none());
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Info(message)) if message.contains("Published draw.io file")
        ));
    }

    #[test]
    fn diagram_drawio_workflow_visual_change_during_export_picker_prevents_publication() {
        let directory = tempfile::tempdir().expect("temporary stale draw.io export directory");
        let destination = directory.path().join("stale.drawio");
        let mut dialog = initialized_visual_dialog();
        dialog
            .documents
            .replace_visual_label("Review snapshot")
            .expect("set initial title");
        let initial_revision = dialog.documents.visual_revision().unwrap();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }

        click_visual_drawio_export(&mut dialog, &ctx);
        wait_for_drawio_review(&mut dialog, &ctx);
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        dialog.test_set_picker_visual_label_change("Changed while choosing destination");
        click_action(&mut dialog, &ctx, "drawio_publish_export");

        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert_eq!(
            dialog.documents.visual_revision(),
            Some(initial_revision + 1)
        );
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .and_then(|scene| scene.label()),
            Some("Changed while choosing destination")
        );
        assert!(!destination.exists());
        assert!(matches!(
            &dialog.message,
            Some(StudioNotice::Error(message)) if message.contains("while choosing")
        ));
    }

    #[test]
    fn diagram_studio_visual_asset_picker_waits_for_accepted_owner_pass_and_cancel_is_nonmutating()
    {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("discarded.png");
        write_test_png(&path, 4, 3);
        let before = dialog.documents.visual_scene().unwrap().clone();
        dialog.test_set_next_file_picker_result(Some(path));
        click_action_with_child_discard(&mut dialog, &ctx, "visual_add_image");
        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert!(dialog.visual_asset_work.is_none());
        assert_eq!(dialog.documents.visual_scene().unwrap(), &before);

        dialog.test_set_next_file_picker_result(None);
        click_action(&mut dialog, &ctx, "visual_add_image");
        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert!(dialog.visual_asset_work.is_none());
        assert_eq!(dialog.documents.visual_scene().unwrap(), &before);
    }

    #[test]
    fn diagram_studio_visual_asset_import_embeds_and_shares_one_decoded_texture() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("local-image.png");
        write_test_png(&path, 32, 20);
        let original_revision = dialog.documents.visual_revision().unwrap();
        dialog.test_set_next_file_picker_result(Some(path.clone()));
        click_action(&mut dialog, &ctx, "visual_add_image");
        assert_eq!(dialog.test_file_picker_calls(), 1);

        for _ in 0..100 {
            let has_reference = dialog.documents.visual_scene().is_some_and(|scene| {
                scene
                    .objects
                    .iter()
                    .any(|object| matches!(object.kind, VisualObjectKind::ImageReference { .. }))
            });
            let texture_ready = dialog.documents.visual_scene().is_some_and(|scene| {
                scene
                    .assets
                    .iter()
                    .all(|asset| dialog.visual_asset_textures.contains_key(asset.id()))
            });
            if has_reference && texture_ready {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            frame(&mut dialog, &ctx);
        }

        let scene = dialog.documents.visual_scene().unwrap();
        assert_eq!(
            dialog.documents.visual_revision(),
            Some(original_revision + 1)
        );
        assert_eq!(scene.assets.len(), 1);
        assert!(
            scene
                .objects
                .iter()
                .any(|object| matches!(object.kind, VisualObjectKind::ImageReference { .. }))
        );
        assert_eq!(dialog.visual_asset_textures.len(), 1);
        assert!(dialog.documents.is_dirty());
        assert!(dialog.documents.session().is_none());

        dialog.test_set_next_file_picker_result(Some(path.clone()));
        click_action(&mut dialog, &ctx, "visual_add_image");
        assert_eq!(dialog.test_file_picker_calls(), 2);
        for _ in 0..100 {
            let references = dialog
                .documents
                .visual_scene()
                .unwrap()
                .objects
                .iter()
                .filter(|object| matches!(object.kind, VisualObjectKind::ImageReference { .. }))
                .count();
            if references == 2 && dialog.visual_asset_work.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            frame(&mut dialog, &ctx);
        }
        assert_eq!(dialog.documents.visual_scene().unwrap().assets.len(), 1);
        assert_eq!(dialog.visual_asset_textures.len(), 1);
        assert!(dialog.visual_asset_pixels.is_empty());

        let saved_path = temp.path().join("embedded-image.mld");
        dialog.documents.save_as(&saved_path).unwrap();
        assert!(!dialog.documents.is_dirty());
        let saved = std::fs::read_to_string(&saved_path).unwrap();
        assert!(!saved.contains(path.to_str().unwrap()));

        open_viewer(&mut dialog, &ctx);
        assert_eq!(dialog.visual_asset_textures.len(), 1);
        assert_eq!(dialog.test_viewer_visual_projection_items, 3);
    }

    #[test]
    fn diagram_studio_visual_component_preset_copies_selected_primitives_and_uses_uniform_instance()
    {
        let mut dialog = initialized_visual_dialog();
        let mut first = VisualObjectDraft {
            layer_id: DEFAULT_LAYER_ID,
            transform: VisualTransform {
                x: -80.0,
                y: 20.0,
                width: 40.0,
                height: 20.0,
                rotation_degrees: 35.0,
            },
            style: VisualStyle::default(),
            kind: VisualObjectKind::Primitive {
                shape: VisualPrimitiveShape::Diamond,
            },
        };
        first.style.border = VisualColor::rgba(166, 30, 55, 255);
        let second = VisualObjectDraft {
            layer_id: DEFAULT_LAYER_ID,
            transform: VisualTransform {
                x: 80.0,
                y: 20.0,
                width: 30.0,
                height: 15.0,
                rotation_degrees: -10.0,
            },
            style: VisualStyle::default(),
            kind: VisualObjectKind::Primitive {
                shape: VisualPrimitiveShape::Circle,
            },
        };
        let outcome = dialog
            .documents
            .apply_visual_transaction(&[
                VisualOperation::AddObject(first),
                VisualOperation::AddObject(second),
            ])
            .unwrap();
        let originals = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .filter(|object| outcome.created_objects.contains(&object.id))
            .cloned()
            .collect::<Vec<_>>();
        dialog.set_visual_selection(VisualSelectionState {
            objects: outcome.created_objects.iter().copied().collect(),
            primary: outcome.created_objects.last().copied(),
        });
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut dialog, &ctx);
        }
        let revision = dialog.documents.visual_revision().unwrap();
        click_action(&mut dialog, &ctx, "visual_create_preset");
        assert_eq!(dialog.documents.visual_revision(), Some(revision + 1));
        let preset = dialog.documents.visual_scene().unwrap().presets[0].clone();
        assert_eq!(preset.components.len(), 2);
        assert_eq!(preset.components[0].shape, VisualPrimitiveShape::Diamond);
        assert_eq!(
            preset.components[0].style.border,
            VisualColor::rgba(166, 30, 55, 255)
        );
        assert_eq!(
            dialog
                .documents
                .visual_scene()
                .unwrap()
                .objects
                .iter()
                .filter(|object| outcome.created_objects.contains(&object.id))
                .cloned()
                .collect::<Vec<_>>(),
            originals
        );

        click_action(&mut dialog, &ctx, "visual_add_preset_instance");
        let instance = dialog
            .documents
            .visual_scene()
            .unwrap()
            .objects
            .iter()
            .find(|object| matches!(object.kind, VisualObjectKind::PresetInstance { .. }))
            .unwrap();
        assert_eq!(instance.transform.width, instance.transform.height);
        let projection = dialog.visual_projection().unwrap().1.unwrap();
        assert!(projection.items.iter().any(|item| {
            item.kind == VisualProjectedKind::PresetPrimitive(VisualPrimitiveShape::Diamond)
        }));
    }

    #[test]
    fn diagram_studio_visual_asset_worker_result_is_retired_after_document_identity_changes() {
        let mut dialog = initialized_visual_dialog();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let prepared =
            crate::diagram::prepare_visual_asset_bytes(bytes.into_inner(), usize::MAX).unwrap();
        let viewport_id = egui::ViewportId::ROOT;
        let (sender, receiver) = sync_channel(1);
        let old_document = dialog.render_document.clone();
        let revision = dialog.documents.visual_revision().unwrap();
        dialog.visual_asset_work = Some(PendingVisualAssetWork {
            document: old_document.clone(),
            revision,
            owner_generation: dialog.owner_generation,
            owner_viewport_id: viewport_id,
            kind: VisualAssetWorkKind::Import(VisualAssetImportFence {
                document: old_document,
                revision,
                owner_generation: dialog.owner_generation,
                owner_viewport_id: viewport_id,
                start_pass: ViewportPassProvenance::current(&ctx, viewport_id),
                start_pass_accepted: true,
            }),
            cancelled: Arc::new(AtomicBool::new(false)),
            receiver,
            hydration_ids: Vec::new(),
            retired: false,
        });

        dialog.reset_document_presentation();
        assert!(
            dialog
                .visual_asset_work
                .as_ref()
                .is_some_and(|work| work.retired)
        );
        sender
            .send(VisualAssetWorkResult::Import(Ok(prepared)))
            .unwrap();
        dialog.poll_visual_asset_work();
        assert!(dialog.visual_asset_work.is_none());
        assert!(dialog.completed_visual_asset_import.is_none());
        assert!(dialog.documents.visual_scene().unwrap().assets.is_empty());
        assert_eq!(dialog.documents.visual_revision(), Some(revision));
    }

    #[test]
    fn mermaid_visual_copy_control_publishes_a_distinct_visual_document() {
        let directory = tempfile::tempdir().unwrap();
        let recents_path = directory.path().join("diagram-recents.json");
        let destination = directory.path().join("flowchart-visual.mld");
        let source = "flowchart LR\nA[\"東京&#10;Caption\"] -->|stores| B{\"Choice\"}\nB --> C(\"Cached\")\n";
        let mut dialog = DiagramStudioDialog::new_with_recent_index(
            directory.path().to_path_buf(),
            Some(recents_path.clone()),
        );
        dialog.test_replace_source(source);
        dialog.open();
        let ctx = egui::Context::default();
        frame(&mut dialog, &ctx);
        assert_eq!(
            dialog.test_action_enabled("convert_visual_copy"),
            Some(true)
        );
        let original_identity = dialog.render_document.clone();
        let original_revision = dialog.test_session().revision();
        let original_draft = dialog.test_session().draft().to_owned();
        click_action_with_child_discard(&mut dialog, &ctx, "convert_visual_copy");
        assert_eq!(dialog.test_file_picker_calls(), 0);
        assert!(!destination.exists());
        assert_eq!(dialog.test_session().draft(), original_draft);
        dialog.test_set_next_file_picker_result(Some(destination.clone()));

        click_action(&mut dialog, &ctx, "convert_visual_copy");

        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert_eq!(dialog.render_document, original_identity);
        assert_eq!(dialog.test_session().revision(), original_revision);
        assert_eq!(dialog.test_session().draft(), original_draft);
        assert!(dialog.documents.is_dirty());
        assert_eq!(dialog.documents.path(), None);
        let mut opened = DiagramDocumentController::new();
        assert_eq!(
            opened.request_open(&destination).unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(
            opened.authoring_kind(),
            Some(crate::diagram::DiagramAuthoringKind::Visual)
        );
        assert!(opened.visual_scene().unwrap().objects.len() >= 7);
        assert_eq!(
            RecentDiagramIndex::load(&recents_path).unwrap().paths(),
            [destination]
        );
    }

    #[test]
    fn mermaid_visual_copy_rejects_grouped_sources_and_stale_picker_results() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut grouped = DiagramStudioDialog::default();
        grouped.test_replace_source("flowchart TD\nsubgraph G[\"Group\"]\nA\nend\n");
        grouped.open();
        frame(&mut grouped, &ctx);
        assert_eq!(
            grouped.test_action_enabled("convert_visual_copy"),
            Some(false)
        );
        assert_eq!(grouped.test_file_picker_calls(), 0);

        let destination = directory.path().join("stale-visual.mld");
        let mut dialog = DiagramStudioDialog::default();
        dialog.test_replace_source("flowchart TD\nA[\"Before picker\"]\n");
        dialog.open();
        dialog.test_set_picker_source_change("flowchart TD\nB[\"After picker\"]\n");
        dialog.test_set_next_file_picker_result(Some(destination.clone()));
        click_action(&mut dialog, &ctx, "convert_visual_copy");

        assert_eq!(dialog.test_file_picker_calls(), 1);
        assert!(
            !destination.exists(),
            "stale source must not publish the copy"
        );
        assert!(dialog.test_session().draft().contains("After picker"));
    }

    #[test]
    fn diagram_recent_panel_and_open_use_accepted_dirty_document_transitions() {
        let directory = tempfile::tempdir().unwrap();
        let recents_path = directory.path().join("diagram-recents.json");
        let target = directory.path().join("target.mld");
        let other = directory.path().join("other.mld");
        save_test_diagram(&target, "flowchart TD\nTarget[\"Saved target\"]\n");
        save_test_diagram(&other, "flowchart LR\nOther[\"Other\"]\n");
        let mut index = RecentDiagramIndex::default();
        index.record(&target).unwrap();
        index.record(&other).unwrap();
        index.save(&recents_path).unwrap();

        let mut dialog = DiagramStudioDialog::new_with_recent_index(
            directory.path().to_path_buf(),
            Some(recents_path),
        );
        dialog.test_replace_source("flowchart TD\nUnsaved[\"Keep until decision\"]\n");
        dialog.open();
        let ctx = egui::Context::default();
        click_action_with_child_discard(&mut dialog, &ctx, "recent_panel");
        assert!(!dialog.recent_panel_open);
        assert!(
            !dialog.recent_index_loaded,
            "a discarded panel pass does not load recents"
        );
        click_action(&mut dialog, &ctx, "recent_panel");
        assert!(dialog.recent_panel_open);
        assert!(dialog.recent_index_loaded);

        let original = dialog.test_session().draft().to_owned();
        click_action_with_child_discard(&mut dialog, &ctx, "recent_open_1");
        assert!(dialog.documents.pending_transition().is_none());
        assert_eq!(dialog.test_session().draft(), original);
        click_action(&mut dialog, &ctx, "recent_open_1");
        assert!(matches!(
            dialog.documents.pending_transition(),
            Some(PendingDiagramTransition::Open(path)) if path == &target
        ));
        assert_eq!(dialog.test_session().draft(), original);
        click_action(&mut dialog, &ctx, "decision_discard");
        assert_eq!(dialog.documents.path(), Some(target.as_path()));
        assert!(dialog.test_session().draft().contains("Saved target"));
        assert_eq!(
            dialog.recent_index.as_ref().unwrap().paths().first(),
            Some(&target),
            "the applied open moves its path to the newest position"
        );
    }

    #[test]
    fn diagram_recent_duplicate_preserves_bytes_and_does_not_replace_active_document() {
        let directory = tempfile::tempdir().unwrap();
        let recents_path = directory.path().join("diagram-recents.json");
        let source = directory.path().join("saved-mermaid.mld");
        let destination = directory.path().join("saved-mermaid-copy.mld");
        let original_bytes = save_test_diagram(&source, "flowchart TD\nA[\"Saved bytes\"]\n");
        let mut index = RecentDiagramIndex::default();
        index.record(&source).unwrap();
        index.save(&recents_path).unwrap();

        let mut dialog = DiagramStudioDialog::new_with_recent_index(
            directory.path().to_path_buf(),
            Some(recents_path.clone()),
        );
        dialog.test_replace_source("flowchart LR\nActive[\"Keep active\"]\n");
        dialog.open();
        let ctx = egui::Context::default();
        click_action(&mut dialog, &ctx, "recent_panel");
        let original_identity = dialog.render_document.clone();
        let original_revision = dialog.test_session().revision();
        let original_draft = dialog.test_session().draft().to_owned();
        dialog.test_set_next_file_picker_result(Some(destination.clone()));

        click_action(&mut dialog, &ctx, "recent_duplicate_0");

        assert_eq!(std::fs::read(&destination).unwrap(), original_bytes);
        assert_eq!(dialog.render_document, original_identity);
        assert_eq!(dialog.test_session().revision(), original_revision);
        assert_eq!(dialog.test_session().draft(), original_draft);
        assert!(dialog.documents.pending_transition().is_none());
        assert_eq!(
            RecentDiagramIndex::load(&recents_path)
                .unwrap()
                .paths()
                .first(),
            Some(&destination)
        );
    }

    #[test]
    fn diagram_recent_corrupt_index_is_preserved_after_a_successful_save_as() {
        let directory = tempfile::tempdir().unwrap();
        let recents_path = directory.path().join("diagram-recents.json");
        let document_path = directory.path().join("saved.mld");
        let corrupt = br#"{"version":99,"paths":["keep this opaque"]}"#.to_vec();
        std::fs::write(&recents_path, &corrupt).unwrap();
        let mut dialog = DiagramStudioDialog::new_with_recent_index(
            directory.path().to_path_buf(),
            Some(recents_path.clone()),
        );
        dialog.test_replace_source("flowchart TD\nA\n");

        dialog.save_as_to(document_path.clone());

        assert!(document_path.is_file());
        assert_eq!(std::fs::read(&recents_path).unwrap(), corrupt);
        assert_eq!(dialog.documents.path(), Some(document_path.as_path()));
        assert!(dialog.message.as_ref().is_some_and(|message| {
            message
                .text()
                .contains("existing recent list was left unchanged")
        }));
    }
}
