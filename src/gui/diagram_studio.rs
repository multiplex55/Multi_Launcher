#[cfg(windows)]
use super::native_keyboard::reserve_windows_system_menu_alt_space;
use crate::diagram::{
    DiagramDocument, DiagramDocumentController, DiagramDocumentError, DiagramExportFormat,
    DiagramImageSnapshot, DiagramScene, DiagramSession, DiagramSourceCopy, DiagramSourceCopyOrigin,
    DiagramSourceSnapshot, DiagramTemplate, EdgeId, FlowDirection, GraphOperation, GroupId,
    MAX_DIAGRAM_GROUPS, MAX_DIAGRAM_NODES, MermaidFile, NodeId, NodePosition, NodeShape,
    PendingDiagramTransition, Position, PreparedText, PreviewState, SceneMetrics, ScenePoint,
    SceneRect, SessionStatus, TextHorizontalAlignment, TransitionDecision, TransitionOutcome,
    build_scene,
};
use eframe::egui;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

const DEFAULT_VIEWPORT_SIZE: [f32; 2] = [1180.0, 760.0];
const MIN_VIEWPORT_SIZE: [f32; 2] = [480.0, 360.0];
const CAMERA_MIN_ZOOM: f32 = 0.025;
const CAMERA_MAX_ZOOM: f32 = 8.0;
const CAMERA_KEYBOARD_PAN_SCREEN_POINTS: f64 = 64.0;
const CAMERA_KEYBOARD_ZOOM_FACTOR: f32 = 1.2;
const GRID_SPACING: f32 = 10.0;
const DRAG_NODE_SEPARATION: f64 = 120.0;
const VISUAL_TWO_COLUMN_MIN_WIDTH: f32 = 460.0;
const SPLIT_TWO_COLUMN_MIN_WIDTH: f32 = 860.0;
const NATIVE_DIAGRAM_EXTENSION: &str = "mld";

pub(super) fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("diagram_studio_viewport")
}

fn viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Diagram Studio")
        .with_inner_size(DEFAULT_VIEWPORT_SIZE)
        .with_min_inner_size(MIN_VIEWPORT_SIZE)
        .with_resizable(true)
        .with_visible(true)
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
    Source,
    Visual,
    #[default]
    Split,
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

impl ViewMode {
    const ALL: [(Self, &'static str); 3] = [
        (Self::Source, "Source"),
        (Self::Visual, "Visual"),
        (Self::Split, "Split"),
    ];
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

enum PendingMutation {
    ReplaceSource(String),
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
    Open,
    ImportMermaid,
    ApplyTemplate(DiagramTemplate),
    AutoLayout,
    Save,
    SaveAs,
    Export(DiagramExportFormat, ExportScale),
    UseSourceInMarkdown(DiagramSourceCopyOrigin),
    Close,
    ResolvePending(TransitionDecision),
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
    active_menu_scope: Option<MenuFocusScope>,
}

struct PendingRootEffects {
    root_frame_nr: u64,
    root_pass_nr: u64,
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
    menu_scope_observed: bool,
    active_menu_scope: Option<MenuFocusScope>,
    mutation: Option<PendingMutation>,
    document_intent: Option<DocumentIntent>,
}

pub(crate) struct DiagramStudioDialog {
    pub(crate) open: bool,
    viewport_bootstrap_pending: bool,
    focus_viewport: bool,
    focus_source: bool,
    close_pending: bool,
    pending_root_effects: Option<PendingRootEffects>,
    deferred_canvas_focus: Option<(egui::ViewportId, egui::Id)>,
    deferred_source_focus: Option<(egui::ViewportId, egui::Id)>,
    deferred_menu_focus: Option<(egui::ViewportId, MenuFocusRequest)>,
    active_menu_scope: Option<MenuFocusScope>,
    documents: DiagramDocumentController,
    pending_markdown_source: Option<DiagramMarkdownSource>,
    default_directory: PathBuf,
    view_mode: ViewMode,
    export_scale: ExportScale,
    cache: Option<RevisionCache>,
    camera: Camera,
    selection: Option<DiagramElement>,
    selected_nodes: BTreeSet<NodeId>,
    property_draft: Option<PropertyDraft>,
    drag: Option<DragState>,
    message: Option<StudioNotice>,
    #[cfg(test)]
    test_source_editor_id: Option<egui::Id>,
    #[cfg(test)]
    test_source_editor_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_canvas_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_canvas_id: Option<egui::Id>,
    #[cfg(test)]
    test_action_rects: Vec<(String, egui::Rect, egui::Rect)>,
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
    test_application_input_allowed: bool,
    #[cfg(test)]
    test_child_focused: bool,
    #[cfg(test)]
    test_request_child_discard: bool,
    #[cfg(test)]
    test_child_discard_requested: bool,
    #[cfg(test)]
    test_pending_mutation: Option<PendingMutation>,
    #[cfg(test)]
    test_pending_document_intent: Option<DocumentIntent>,
    #[cfg(test)]
    test_next_file_picker_result: Option<Option<PathBuf>>,
    #[cfg(test)]
    test_file_picker_calls: usize,
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
        Self::new_with_directory(root.path().to_path_buf())
    }

    fn new_with_directory(default_directory: PathBuf) -> Self {
        Self {
            open: false,
            viewport_bootstrap_pending: false,
            focus_viewport: false,
            focus_source: false,
            close_pending: false,
            pending_root_effects: None,
            deferred_canvas_focus: None,
            deferred_source_focus: None,
            deferred_menu_focus: None,
            active_menu_scope: None,
            documents: DiagramDocumentController::new(),
            pending_markdown_source: None,
            default_directory,
            view_mode: ViewMode::Split,
            export_scale: ExportScale::default(),
            cache: None,
            camera: Camera::default(),
            selection: None,
            selected_nodes: BTreeSet::new(),
            property_draft: None,
            drag: None,
            message: None,
            #[cfg(test)]
            test_source_editor_id: None,
            #[cfg(test)]
            test_source_editor_rect: None,
            #[cfg(test)]
            test_canvas_rect: None,
            #[cfg(test)]
            test_canvas_id: None,
            #[cfg(test)]
            test_action_rects: Vec::new(),
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
            test_application_input_allowed: false,
            #[cfg(test)]
            test_child_focused: false,
            #[cfg(test)]
            test_request_child_discard: false,
            #[cfg(test)]
            test_child_discard_requested: false,
            #[cfg(test)]
            test_pending_mutation: None,
            #[cfg(test)]
            test_pending_document_intent: None,
            #[cfg(test)]
            test_next_file_picker_result: None,
            #[cfg(test)]
            test_file_picker_calls: 0,
        }
    }

    /// Explicit opener; repeated user opens focus the existing child.
    pub(crate) fn open(&mut self) {
        if !self.ensure_blank_after_close() {
            return;
        }
        if !self.open {
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
        self.deferred_menu_focus = None;
        self.active_menu_scope = None;
    }

    /// Pinned-panel maintenance may show the child but does not steal focus.
    pub(crate) fn ensure_open(&mut self) {
        // Passive maintenance must not resurrect a document accepted as
        // closed. A later explicit opener creates the next blank session.
        if self.documents.session().is_none() {
            return;
        }
        if !self.open {
            self.viewport_bootstrap_pending = true;
        }
        self.open = true;
    }

    pub(crate) fn needs_viewport_bootstrap(&self) -> bool {
        self.viewport_bootstrap_pending
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        self.viewport_bootstrap_pending || self.close_pending
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
        if self.documents.session().is_some() {
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
        self.open = false;
        self.close_pending = false;
        self.viewport_bootstrap_pending = false;
        self.focus_viewport = false;
        self.focus_source = false;
        self.deferred_canvas_focus = None;
        self.deferred_source_focus = None;
        self.deferred_menu_focus = None;
        self.active_menu_scope = None;
        self.reset_document_presentation();
    }

    fn cancel_transients(&mut self) {
        self.drag = None;
        self.property_draft = None;
    }

    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_input_allowed: bool,
    ) {
        #[cfg(feature = "qa-framebuffer-capture")]
        self.show_impl(
            ctx,
            first_ui_pass,
            application_input_allowed,
            root_input_allowed,
            None,
        );
        #[cfg(not(feature = "qa-framebuffer-capture"))]
        self.show_impl(
            ctx,
            first_ui_pass,
            application_input_allowed,
            root_input_allowed,
        );
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(super) fn show_with_observation(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_input_allowed: bool,
    ) -> Option<DiagramViewportObservation> {
        let mut observation = None;
        self.show_impl(
            ctx,
            first_ui_pass,
            application_input_allowed,
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
        root_input_allowed: bool,
        #[cfg(feature = "qa-framebuffer-capture")] mut observation_slot: Option<
            &mut Option<DiagramViewportObservation>,
        >,
    ) {
        self.refresh_cache();
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
        let mut active_menu_scope = None;
        let mut document_intent = None;
        let mut body_rendered = false;
        let mut child_modal_open = false;
        let mut child_focused = false;
        let mut callback_class = egui::ViewportClass::Root;
        let mut child_viewport_id = None;
        self.test_begin_frame();
        ctx.show_viewport_immediate(viewport_id(), viewport_builder(), |child, class| {
            callback_class = class;
            let independent = class == egui::ViewportClass::Immediate;
            let child_context = child.ctx().clone();
            child_viewport_id = Some(child_context.viewport_id());
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
            let child_input_allowed = surface_input_allowed && focused && !modal_open;
            let actions_allowed = pass_authorized && child_input_allowed;
            let source_focus_allowed = pass_authorized && surface_input_allowed && !modal_open;
            focus_requested = independent
                && self.focus_viewport
                && pass_authorized
                && application_input_allowed
                && !modal_open;

            let popup_open = egui::Popup::is_any_open(&child);
            let keyboard_close = if actions_allowed {
                child.input_mut(|input| {
                    (!popup_open && input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                        || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                })
            } else {
                false
            };
            let keyboard_document_intent = if actions_allowed
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

            if independent {
                egui::CentralPanel::default().show(child, |ui| {
                    body_rendered = ui.is_visible() && !ui.is_sizing_pass();
                    let effects = self.draw_body(
                        ui,
                        actions_allowed,
                        close_requested,
                        &mut pending,
                        &mut document_intent,
                        &mut canvas_focus_requested,
                        &mut source_focus_requested,
                        source_focus_allowed,
                    );
                    menu_focus_request = effects.menu_focus_request;
                    export_scale_requested = effects.export_scale_requested;
                    active_menu_scope = effects.active_menu_scope;
                });
            } else {
                body_rendered = child.is_visible() && !child.is_sizing_pass();
                let effects = self.draw_body(
                    child,
                    actions_allowed,
                    close_requested,
                    &mut pending,
                    &mut document_intent,
                    &mut canvas_focus_requested,
                    &mut source_focus_requested,
                    source_focus_allowed,
                );
                menu_focus_request = effects.menu_focus_request;
                export_scale_requested = effects.export_scale_requested;
                active_menu_scope = effects.active_menu_scope;
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
                if let Some((target_viewport, id)) = self.deferred_source_focus
                    && target_viewport == child_context.viewport_id()
                {
                    self.deferred_source_focus = None;
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

        if first_ui_pass {
            let viewport_callback_observed = child_viewport_id.is_some();
            self.pending_root_effects = Some(PendingRootEffects {
                root_frame_nr: ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT),
                root_pass_nr: ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT),
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
                menu_scope_observed: child_pass_accepted && body_rendered,
                active_menu_scope: (child_pass_accepted && body_rendered)
                    .then_some(active_menu_scope)
                    .flatten(),
                mutation: (mutation_authorized && child_pass_accepted)
                    .then_some(pending)
                    .flatten(),
                document_intent: (child_pass_accepted && document_intent_authorized)
                    .then_some(document_intent)
                    .flatten(),
            });
        }
        #[cfg(test)]
        {
            self.test_body_rendered = body_rendered;
            self.test_child_focused = child_focused;
            let _ = child_modal_open;
        }
        let _ = (body_rendered, child_focused, child_modal_open);
    }

    pub(crate) fn on_ui_frame_complete(&mut self, ctx: &egui::Context) -> bool {
        let Some(effects) = self.pending_root_effects.take() else {
            return false;
        };
        let retained_pass = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT)
            == effects.root_frame_nr.saturating_add(1)
            && ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT)
                == effects.root_pass_nr.saturating_add(1);
        if !retained_pass {
            if effects.os_close_requested {
                ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::CancelClose);
            }
            return false;
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
            || effects.mutation.is_some()
            || effects.close_dialog
            || effects.close_viewport
            || effects.document_intent.is_some()
            || effects.os_close_requested;
        let repaint_root = bootstrap_completed || repaint_child;

        if let Some(target_viewport) = child_viewport_id.clone() {
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
        if effects.focus_viewport {
            ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Focus);
            self.focus_viewport = false;
        }
        let mutation_succeeded = effects
            .mutation
            .is_none_or(|mutation| self.apply_pending(mutation));
        let accepted_close = mutation_succeeded
            && effects
                .document_intent
                .is_some_and(|intent| self.apply_document_intent(intent));
        if effects.close_viewport && accepted_close {
            ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Close);
        } else if effects.os_close_requested && !accepted_close {
            ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::CancelClose);
        }
        if effects.close_dialog {
            self.close_pending = false;
        }
        if repaint_child {
            if let Some(target_viewport) = child_viewport_id {
                ctx.request_repaint_of(target_viewport);
            }
        }
        if repaint_root {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        accepted_close
    }

    fn apply_document_intent(&mut self, intent: DocumentIntent) -> bool {
        if self.documents.pending_transition().is_some()
            && matches!(
                &intent,
                DocumentIntent::ImportMermaid
                    | DocumentIntent::ApplyTemplate(_)
                    | DocumentIntent::AutoLayout
                    | DocumentIntent::Export(_, _)
                    | DocumentIntent::UseSourceInMarkdown(_)
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
            DocumentIntent::Open => {
                if let Some(path) = self.pick_native_document(false) {
                    match self.documents.request_open(&path) {
                        Ok(outcome) => {
                            self.finish_transition(outcome, false);
                        }
                        Err(error) => {
                            self.show_document_error("open", Some(&path), error);
                        }
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
                Ok(()) => self.message = Some(StudioNotice::info("Diagram saved.")),
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
                self.export_document(format, scale);
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
                match self.documents.resolve_pending(decision) {
                    Ok(outcome) => {
                        let closes = pending == Some(PendingDiagramTransition::Close);
                        let closed = self.finish_transition(outcome, closes);
                        accepted_close = closes && closed;
                    }
                    Err(DiagramDocumentError::SaveAsRequired)
                        if decision == TransitionDecision::Save =>
                    {
                        if let Some(path) = self.pick_native_document(true) {
                            accepted_close = match self.documents.resolve_pending_save_as(&path) {
                                Ok(outcome) => {
                                    let closes = pending == Some(PendingDiagramTransition::Close);
                                    let closed = self.finish_transition(outcome, closes);
                                    closes && closed
                                }
                                Err(error) => {
                                    self.show_document_error("save", Some(&path), error);
                                    false
                                }
                            };
                        }
                    }
                    Err(error) => {
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
            Ok(()) => self.message = Some(StudioNotice::info(format!("Saved {}.", path.display()))),
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
            DiagramExportFormat::Svg => {
                DiagramImageSnapshot::capture(session, SceneMetrics::default())
                    .and_then(|snapshot| snapshot.encode_svg(scale.factor()))
            }
            DiagramExportFormat::Png => {
                DiagramImageSnapshot::capture(session, SceneMetrics::default())
                    .and_then(|snapshot| snapshot.encode_png(scale.factor()))
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
        let Some(path) = self.pick_export_destination(format) else {
            self.message = Some(StudioNotice::info(format!(
                "{} export cancelled.",
                export_format_name(format)
            )));
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

    fn pick_mermaid_source(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_file_picker_calls = self.test_file_picker_calls.saturating_add(1);
            return self.test_next_file_picker_result.take().unwrap_or(None);
        }
        #[cfg(not(test))]
        {
            let directory = self.picker_directory();
            let mut picker = rfd::FileDialog::new().add_filter("Mermaid source (*.mmd)", &["mmd"]);
            if directory.is_dir() {
                picker = picker.set_directory(directory);
            }
            picker.pick_file()
        }
    }

    fn pick_export_destination(&mut self, format: DiagramExportFormat) -> Option<PathBuf> {
        #[cfg(test)]
        {
            self.test_file_picker_calls = self.test_file_picker_calls.saturating_add(1);
            return self
                .test_next_file_picker_result
                .take()
                .unwrap_or(None)
                .map(|path| ensure_export_extension(path, format));
        }
        #[cfg(not(test))]
        {
            let directory = self.picker_directory();
            let default_name = self
                .documents
                .path()
                .and_then(Path::file_stem)
                .and_then(|name| name.to_str())
                .unwrap_or("diagram");
            let file_name = format!("{default_name}.{}", format.extension());
            let mut picker = rfd::FileDialog::new()
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
            let current_directory = self
                .documents
                .path()
                .and_then(Path::parent)
                .filter(|directory| directory.is_dir());
            let directory = current_directory.unwrap_or(self.default_directory.as_path());
            let mut picker = rfd::FileDialog::new()
                .add_filter("Diagram Studio document", &[NATIVE_DIAGRAM_EXTENSION]);
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

    fn test_begin_frame(&mut self) {
        #[cfg(test)]
        {
            self.test_source_editor_rect = None;
            self.test_canvas_rect = None;
            self.test_canvas_id = None;
            self.test_action_rects.clear();
            self.test_action_focus_ids.clear();
            self.test_discard_button_id = None;
            self.test_body_rendered = false;
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
        let (scene, scene_error) = session.preview_graph().map_or((None, None), |graph| {
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
        let result = match pending {
            PendingMutation::ReplaceSource(source) => self
                .documents
                .replace_source(source)
                .map(|changed| (changed, None)),
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

    #[cfg(test)]
    pub(crate) fn test_replace_source(&mut self, source: &str) {
        let _ = self.documents.replace_source(source);
        self.refresh_cache();
    }

    fn reset_document_presentation(&mut self) {
        self.cache = None;
        self.selection = None;
        self.selected_nodes.clear();
        self.property_draft = None;
        self.drag = None;
        self.camera = Camera::default();
        self.message = None;
        self.deferred_canvas_focus = None;
        self.deferred_source_focus = None;
        self.deferred_menu_focus = None;
        self.active_menu_scope = None;
        self.refresh_cache();
    }

    #[cfg(test)]
    pub(crate) fn test_request_child_discard(&mut self) {
        self.test_request_child_discard = true;
    }

    #[cfg(test)]
    fn test_canvas_has_focus(&self) -> bool {
        self.test_canvas_id.is_some_and(|id| {
            super::render::immediate_viewport_test_focused_id()
                .is_some_and(|(viewport, focused)| viewport == viewport_id() && focused == Some(id))
        })
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
        self.documents.session().is_some()
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
    pub(crate) fn test_set_next_file_picker_result(&mut self, result: Option<PathBuf>) {
        self.test_next_file_picker_result = Some(result);
    }

    #[cfg(test)]
    pub(crate) fn test_file_picker_calls(&self) -> usize {
        self.test_file_picker_calls
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
        closing_requested: bool,
        pending: &mut Option<PendingMutation>,
        document_intent: &mut Option<DocumentIntent>,
        canvas_focus_requested: &mut Option<egui::Id>,
        source_focus_requested: &mut Option<egui::Id>,
        source_focus_allowed: bool,
    ) -> DrawBodyEffects {
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
        let Self {
            documents,
            view_mode,
            export_scale,
            cache,
            camera,
            selection,
            selected_nodes,
            property_draft,
            drag,
            message,
            focus_source,
            #[cfg(test)]
            test_source_editor_id,
            #[cfg(test)]
            test_source_editor_rect,
            #[cfg(test)]
            test_canvas_rect,
            #[cfg(test)]
            test_canvas_id,
            #[cfg(test)]
            test_action_rects,
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
        let base_editing_allowed =
            actions_allowed && !closing_requested && pending_transition.is_none();

        if base_editing_allowed
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
                    ui.separator();
                    for (_name, label, intent, enabled) in [
                        (
                            "document_new",
                            "New",
                            DocumentIntent::New,
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
                                    || status.preview == PreviewState::Current);
                            let response = menu_ui.add_enabled(enabled, egui::Button::new(label));
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
                    for (mode, label) in ViewMode::ALL {
                        let response = ui.selectable_label(*view_mode == mode, label);
                        if background_actions_allowed && response.clicked() {
                            *view_mode = mode;
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
                    if ui
                        .add_enabled(background_actions_allowed, egui::Button::new("Fit"))
                        .clicked()
                    {
                        camera.fit_next = true;
                    }
                    if ui
                        .add_enabled(background_actions_allowed, egui::Button::new("Reset view"))
                        .clicked()
                    {
                        *camera = Camera::default();
                        camera.fit_next = false;
                    }
                });

                let menu_open = active_menu_scope.is_some();
                let editing_allowed = base_editing_allowed && !menu_open;
                let source_edit_allowed =
                    actions_allowed && pending_transition.is_none() && !menu_open;
                let visual_mutations_allowed =
                    editing_allowed && status.preview == PreviewState::Current;
                let decision_actions_allowed = actions_allowed && !closing_requested && !menu_open;

                ui.horizontal_wrapped(|ui| {
                    let add_node = ui.add_enabled(
                        visual_mutations_allowed
                            && graph.is_some_and(|graph| graph.nodes.len() < MAX_DIAGRAM_NODES),
                        egui::Button::new("Add node"),
                    );
                    #[cfg(test)]
                    test_action_rects.push(("add_node".into(), add_node.rect, ui.clip_rect()));
                    if add_node.clicked() && pending.is_none() {
                        if let Some(graph) = graph {
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
                        && (selected_nodes.len() > 0
                            || selection.as_ref().is_some_and(|selected| {
                                !matches!(selected, DiagramElement::Node(_))
                            }));
                    if ui
                        .add_enabled(can_remove, egui::Button::new("Remove selection"))
                        .clicked()
                        && pending.is_none()
                    {
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
                    let auto_layout =
                        ui.add_enabled(visual_mutations_allowed, egui::Button::new("Auto-layout"));
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
                        status.preview == PreviewState::Current,
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

                if let Some(scope) = active_menu_scope.as_ref() {
                    if actions_allowed
                        && !closing_requested
                        && !scope.sizing_pass
                        && !scope.item_ids.is_empty()
                    {
                        let navigation = ui.input_mut(|input| {
                            if consume_menu_key(input, egui::Key::Tab, egui::Modifiers::SHIFT) {
                                Some(false)
                            } else if consume_menu_key(input, egui::Key::Tab, egui::Modifiers::NONE)
                                || consume_menu_key(
                                    input,
                                    egui::Key::ArrowDown,
                                    egui::Modifiers::NONE,
                                )
                                || consume_menu_key(
                                    input,
                                    egui::Key::ArrowRight,
                                    egui::Modifiers::NONE,
                                )
                            {
                                Some(true)
                            } else if consume_menu_key(
                                input,
                                egui::Key::ArrowUp,
                                egui::Modifiers::NONE,
                            ) || consume_menu_key(
                                input,
                                egui::Key::ArrowLeft,
                                egui::Modifiers::NONE,
                            ) {
                                Some(false)
                            } else {
                                None
                            }
                        });
                        if navigation.is_some() {
                            // egui's pass-level focus traversal is computed before
                            // these menu responses are drawn. Clear that traversal
                            // so it cannot move focus behind the popup after the
                            // retained menu target is applied.
                            ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
                        }
                        let focused_index = scope
                            .item_ids
                            .iter()
                            .position(|id| Some(*id) == focused_at_start);
                        let target_index = match (focused_index, navigation) {
                            (Some(index), Some(true)) => Some((index + 1) % scope.item_ids.len()),
                            (Some(index), Some(false)) => Some(if index == 0 {
                                scope.item_ids.len() - 1
                            } else {
                                index - 1
                            }),
                            (None, Some(true)) | (None, None) => Some(0),
                            (None, Some(false)) => Some(scope.item_ids.len() - 1),
                            (Some(_), None) => None,
                        };
                        if let Some(index) = target_index {
                            draw_effects.menu_focus_request = Some(MenuFocusRequest {
                                popup_id: scope.popup_id,
                                destination: MenuFocusDestination::Item(scope.item_ids[index]),
                            });
                        }
                    }
                } else if actions_allowed
                    && ui.input(|input| input.key_pressed(egui::Key::Escape))
                    && let Some(scope) = previous_menu_scope.as_ref()
                {
                    draw_effects.menu_focus_request = Some(MenuFocusRequest {
                        popup_id: scope.popup_id,
                        destination: MenuFocusDestination::Trigger(scope.trigger_id),
                    });
                }
                draw_effects.active_menu_scope = active_menu_scope;

                if let Some(transition) = pending_transition.as_ref() {
                    ui.group(|ui| {
                        let description = match transition {
                            PendingDiagramTransition::New => "starting a new diagram".to_owned(),
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

                if status.preview == PreviewState::Stale {
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
                    ViewMode::Source => source_editor(
                        ui,
                        session,
                        source_edit_allowed,
                        pending,
                        *focus_source && source_focus_allowed,
                        source_focus_requested,
                        #[cfg(test)]
                        test_source_editor_id,
                        #[cfg(test)]
                        test_source_editor_rect,
                    ),
                    ViewMode::Visual => visual_workspace(
                        ui,
                        graph,
                        cache.scene.as_ref(),
                        status.preview == PreviewState::Current,
                        revision,
                        camera,
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
                                source_edit_allowed,
                                pending,
                                *focus_source && source_focus_allowed,
                                source_focus_requested,
                                #[cfg(test)]
                                &mut source_id,
                                #[cfg(test)]
                                &mut source_rect,
                            );
                            ui.separator();
                            visual_workspace(
                                ui,
                                graph,
                                cache.scene.as_ref(),
                                status.preview == PreviewState::Current,
                                revision,
                                camera,
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
                            );
                        } else {
                            ui.columns(2, |columns| {
                                source_editor(
                                    &mut columns[0],
                                    session,
                                    source_edit_allowed,
                                    pending,
                                    *focus_source && source_focus_allowed,
                                    source_focus_requested,
                                    #[cfg(test)]
                                    &mut source_id,
                                    #[cfg(test)]
                                    &mut source_rect,
                                );
                                visual_workspace(
                                    &mut columns[1],
                                    graph,
                                    cache.scene.as_ref(),
                                    status.preview == PreviewState::Current,
                                    revision,
                                    camera,
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
}

fn source_editor(
    ui: &mut egui::Ui,
    session: &DiagramSession,
    actions_allowed: bool,
    pending: &mut Option<PendingMutation>,
    focus_source: bool,
    source_focus_requested: &mut Option<egui::Id>,
    #[cfg(test)] test_editor_id: &mut Option<egui::Id>,
    #[cfg(test)] test_editor_rect: &mut Option<egui::Rect>,
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
    }
    if response.changed() && actions_allowed && pending.is_none() {
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

#[allow(clippy::too_many_arguments)]
fn visual_workspace(
    ui: &mut egui::Ui,
    graph: Option<&DiagramDocument>,
    scene: Option<&DiagramScene>,
    graph_is_current: bool,
    revision: u64,
    camera: &mut Camera,
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
        ui.columns(2, |columns| {
            draw_inspector(
                &mut columns[0],
                graph,
                graph_is_current,
                revision,
                selection,
                selected_nodes,
                property_draft,
                actions_allowed,
                pending,
            );
            let _canvas_rect = columns[1]
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
        });
    }
}

fn canvas_scope_builder() -> egui::UiBuilder {
    egui::UiBuilder::new()
        .id(egui::Id::new("diagram-studio-canvas-scope"))
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
    ui.set_min_width(180.0);
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
    let center_x = snap_world(f64::from(camera_center.x));
    let center_y = snap_world(f64::from(camera_center.y));
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
    pending: &mut Option<PendingMutation>,
    message: &mut Option<StudioNotice>,
    canvas_focus_requested: &mut Option<egui::Id>,
    #[cfg(test)] test_canvas_id: &mut Option<egui::Id>,
) -> egui::Rect {
    let width = ui.available_width().max(220.0);
    let height = ui.available_height().max(300.0);
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
                egui::Id::new(("diagram-group", group.id.as_str())),
                egui::Sense::click(),
            );
            if actions_allowed && response.clicked() {
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
                        egui::Id::new(("diagram-edge", edge.id.get())),
                        egui::Sense::click(),
                    );
                    if actions_allowed && response.clicked() {
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
                    egui::Id::new(("diagram-node", node.id.as_str())),
                    egui::Sense::click_and_drag(),
                );
                if actions_allowed && response.clicked() {
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
                if actions_allowed && response.drag_started() {
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
                    if actions_allowed
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

    if actions_allowed && canvas_response.clicked() {
        *canvas_focus_requested = Some(canvas_id);
        if !canvas_response.drag_started() && !entity_selected {
            *selection = None;
            selected_nodes.clear();
        }
    }
    if actions_allowed {
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

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    const TEST_VIEWPORT_SIZE: egui::Vec2 = egui::vec2(1200.0, 900.0);

    fn frame(dialog: &mut DiagramStudioDialog, ctx: &egui::Context) {
        let _ = frame_with_viewport_commands(dialog, ctx);
    }

    fn frame_with_viewport_commands(
        dialog: &mut DiagramStudioDialog,
        ctx: &egui::Context,
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
                .get(&viewport_id())
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
                assert!(clip.contains_rect(*rect));
                rect.center()
            })
            .expect("the named Diagram Studio control is visible")
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
}
