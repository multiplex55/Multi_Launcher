//! Stable-ID radial menu authoring window.

pub(crate) mod action_editor;
mod appearance;
mod asset_picker;
mod audio_controls;
mod canvas;
mod import_export;
mod preview;
mod skin_editor;
mod work_area;

use crate::gui::LauncherApp;
use crate::gui::jump_mode::ui::{
    FocusDirective, JumpModeScope, JumpModeUi, NavigationDialogKind, NavigationOwner,
    NavigationViewportBinding, RadialDesignerChildKind, radial_designer_child_layer_id,
    radial_designer_child_window_id, register_and_apply_target, visible_top_middle_layer,
};
use crate::gui::jump_mode::{SurfaceId, TargetGeneration, TargetKind};
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, labeled_form_row, secondary_help,
    semantic_status, with_button_emphasis,
};
use crate::radial::acceptance_trace::{
    self, BodyBlock, Correlation, DesignerAuthoringRole, DesignerAuthoringScrollOwner,
    DesignerAuthoringScrollViewport, DesignerAuthoringTarget, DesignerCloseState,
    DesignerGeometryState, DesignerProposalKind, DesignerSemanticRole, DesignerSemanticTarget,
    Event, FocusEdge, RadialInsertionControl, RadialInsertionTraceIdentity, RequestKind,
    ViewportClass, WidgetCategory, WidgetResponse,
};
use crate::radial::authoring::menu::{
    self, BulkAfterActionSlot, BulkCellEdit, BulkCellEditRequest, ResizeResolution,
    SubmenuDuplication,
};
use crate::radial::authoring::{
    AuthoringClient, AuthoringError, AuthoringSnapshot, CloseIntent, CommitDisposition,
    ConflictResolution, DiskSha256, EditKey, EditPhase, PendingRequestKind, RadialAuthoringSession,
    StableSelection,
};
use crate::radial::context::InvocationContext;
use crate::radial::model::{
    ActionBinding, AfterActionPolicy, CellContent, CellId, Control, DynamicSource, InteractionMode,
    LayoutKind, MenuDefinition, MenuId, RadialDocument, RingId, SubmenuPresentation,
};
pub(crate) use action_editor::{
    ActionBindingEditorIntent, ActionBindingEditorState, AuthoringBindingEditorIdentity,
    BindingAssignmentKind, BindingEditorScope, BindingEditorSlot, BindingEditorSurface,
};
pub(crate) use action_editor::{ActionSearchCompletion, ActionSearchCompletionState};
use canvas::{
    CanvasPoint, CanvasTransform, DesignerMode, DesignerNavigationIntent, DragPayload,
    PlacementDraft, PreferenceDebounce, ProjectedCellProvenance, ProjectedSelection,
    SubmenuPathEdge, VisitedMenuPath, WindowGeometry,
};
use eframe::egui;
use import_export::PendingImport;
use preview::{EmbeddedPreview, PreviewPreset};
use std::collections::hash_map::RandomState;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub(crate) fn radial_designer_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("radial-designer")
}

#[derive(Clone, Debug)]
pub(crate) enum HotkeyDirectoryRadialTarget {
    Trigger {
        source_revision: u64,
        trigger_id: String,
        menu_id: String,
    },
    Shortcut {
        source_revision: u64,
        menu_id: String,
        cell_id: String,
        shortcut_id: String,
    },
}

#[derive(Clone, Debug)]
struct PendingHotkeyDirectoryNavigation {
    target: HotkeyDirectoryRadialTarget,
    frames_remaining: u8,
}

fn trace_viewport_class(class: egui::ViewportClass) -> ViewportClass {
    match class {
        egui::ViewportClass::Root => ViewportClass::Root,
        egui::ViewportClass::Deferred => ViewportClass::Deferred,
        egui::ViewportClass::Immediate => ViewportClass::Immediate,
        egui::ViewportClass::EmbeddedWindow => ViewportClass::Embedded,
    }
}

fn trace_request_kind(kind: PendingRequestKind) -> RequestKind {
    match kind {
        PendingRequestKind::Snapshot => RequestKind::Snapshot,
        PendingRequestKind::Commit(CommitDisposition::Apply) => RequestKind::CommitApply,
        PendingRequestKind::Commit(CommitDisposition::Save) => RequestKind::CommitSave,
        PendingRequestKind::Commit(CommitDisposition::RevertAppliedAndClose) => {
            RequestKind::CommitRevertAndClose
        }
        PendingRequestKind::LivePreview => RequestKind::LivePreview,
        PendingRequestKind::CancelPreview => RequestKind::CancelPreview,
        PendingRequestKind::ExportPackage => RequestKind::ExportPackage,
        PendingRequestKind::ExportSkin => RequestKind::ExportSkin,
        PendingRequestKind::AuditionManagedAsset => RequestKind::AuditionManagedAsset,
        PendingRequestKind::FontCatalog => RequestKind::FontCatalog,
        PendingRequestKind::PrepareEmbeddedPreview => RequestKind::PrepareEmbeddedPreview,
        PendingRequestKind::ReplacePackage => RequestKind::ReplacePackage,
        PendingRequestKind::StartNativePreview => RequestKind::StartNativePreview,
        PendingRequestKind::UpdateNativePreview => RequestKind::UpdateNativePreview,
        PendingRequestKind::StopNativePreview => RequestKind::StopNativePreview,
    }
}

fn trace_correlation(session: Option<&RadialAuthoringSession>) -> Correlation {
    let Some(session) = session else {
        return Correlation::default();
    };
    let pending = session.pending_request.or(session.pending_native_preview);
    pending.map_or(
        Correlation {
            session_id: session.editor_session.0,
            generation: session.generation.0,
            terminal: false,
            ..Correlation::default()
        },
        |pending| Correlation {
            request_id: pending.id.0,
            request_kind: trace_request_kind(pending.kind),
            session_id: pending.editor_session.0,
            generation: pending.generation.0,
            terminal: false,
            ..Correlation::default()
        },
    )
}

fn trace_pointer_release_response(
    ui: &egui::Ui,
    response: &egui::Response,
    category: WidgetCategory,
    accepted: bool,
    correlation: Correlation,
) {
    if !acceptance_trace::enabled() {
        return;
    }

    let (pressed, released, pointer_position) = ui.input(|input| {
        (
            input.pointer.any_pressed(),
            input.pointer.any_released(),
            input.pointer.interact_pos(),
        )
    });
    let hovered = response.hovered();
    let button_down_on = response.is_pointer_button_down_on();
    let pointer_inside = pointer_position.is_some_and(|position| response.rect.contains(position));
    if (pressed || released) && (hovered || button_down_on || pointer_inside) {
        let (has_position, pointer_x, pointer_y) = pointer_position
            .map_or((false, i32::MIN, i32::MIN), |position| {
                (true, position.x.round() as i32, position.y.round() as i32)
            });
        let layer_is_topmost = pointer_position
            .is_some_and(|position| ui.ctx().layer_id_at(position) == Some(response.layer_id));
        acceptance_trace::emit(Event::DesignerWidgetPointer {
            category,
            pressed,
            released,
            hovered,
            button_down_on,
            pointer_inside,
            layer_is_topmost,
            clicked: accepted,
            has_position,
            pointer_x,
            pointer_y,
            correlation,
        });
    }

    if released && hovered {
        acceptance_trace::emit(Event::DesignerWidget {
            category,
            response: if accepted {
                WidgetResponse::Accepted
            } else {
                WidgetResponse::Rejected
            },
            correlation,
        });
    }
}

fn trace_designer_semantic_target(
    ui: &egui::Ui,
    response: &egui::Response,
    target: DesignerSemanticTarget,
    role: DesignerSemanticRole,
    viewport: ViewportClass,
    selected: bool,
    correlation: Correlation,
) {
    if !acceptance_trace::enabled() {
        return;
    }
    let pixels_per_point = ui.ctx().pixels_per_point();
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return;
    }
    let rect = response.rect;
    let scale = |coordinate: f32| {
        let scaled = coordinate * pixels_per_point;
        (scaled.is_finite() && scaled >= i32::MIN as f32 && scaled <= i32::MAX as f32)
            .then(|| scaled.round() as i32)
    };
    let Some(left) = scale(rect.left()) else {
        return;
    };
    let Some(top) = scale(rect.top()) else {
        return;
    };
    let Some(right) = scale(rect.right()) else {
        return;
    };
    let Some(bottom) = scale(rect.bottom()) else {
        return;
    };
    if right <= left || bottom <= top {
        return;
    }
    acceptance_trace::emit_designer_semantic_target(
        target,
        role,
        viewport,
        [left, top, right, bottom],
        selected,
        response.has_focus(),
        correlation,
    );
}

fn trace_designer_authoring_control(
    ui: &egui::Ui,
    response: &egui::Response,
    target: DesignerAuthoringTarget,
    role: DesignerAuthoringRole,
    index: Option<usize>,
    enabled: bool,
    selected: bool,
    viewport: ViewportClass,
    correlation: Correlation,
) {
    trace_designer_authoring_control_rect(
        ui,
        response.rect,
        response.clicked(),
        target,
        role,
        index,
        enabled,
        selected,
        response.has_focus(),
        viewport,
        correlation,
        None,
    );
}

fn trace_designer_authoring_control_rect(
    ui: &egui::Ui,
    rect: egui::Rect,
    clicked: bool,
    target: DesignerAuthoringTarget,
    role: DesignerAuthoringRole,
    index: Option<usize>,
    enabled: bool,
    selected: bool,
    focused: bool,
    viewport: ViewportClass,
    correlation: Correlation,
    canvas_scope: Option<acceptance_trace::DesignerCanvasCellScope>,
) {
    trace_designer_authoring_control_scoped_rect(
        ui,
        rect,
        clicked,
        target,
        role,
        index,
        enabled,
        selected,
        focused,
        viewport,
        correlation,
        canvas_scope.map(acceptance_trace::DesignerAuthoringControlScope::CanvasCell),
    );
}

fn trace_designer_authoring_control_with_scope(
    ui: &egui::Ui,
    response: &egui::Response,
    target: DesignerAuthoringTarget,
    role: DesignerAuthoringRole,
    index: Option<usize>,
    enabled: bool,
    selected: bool,
    viewport: ViewportClass,
    correlation: Correlation,
    scope: acceptance_trace::DesignerAuthoringControlScope,
) {
    trace_designer_authoring_control_scoped_rect(
        ui,
        response.rect,
        response.clicked(),
        target,
        role,
        index,
        enabled,
        selected,
        response.has_focus(),
        viewport,
        correlation,
        Some(scope),
    );
}

fn trace_designer_authoring_control_scoped_rect(
    ui: &egui::Ui,
    rect: egui::Rect,
    clicked: bool,
    target: DesignerAuthoringTarget,
    role: DesignerAuthoringRole,
    index: Option<usize>,
    enabled: bool,
    selected: bool,
    focused: bool,
    viewport: ViewportClass,
    correlation: Correlation,
    scope: Option<acceptance_trace::DesignerAuthoringControlScope>,
) {
    if !acceptance_trace::enabled() {
        return;
    }
    let pixels_per_point = ui.ctx().pixels_per_point();
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return;
    }
    let scale = |coordinate: f32| {
        let scaled = coordinate * pixels_per_point;
        (scaled.is_finite() && scaled >= i32::MIN as f32 && scaled <= i32::MAX as f32)
            .then(|| scaled.round() as i32)
    };
    let Some(viewport_rect) = ui.ctx().input(|input| input.viewport().inner_rect) else {
        return;
    };
    let Some(client_width_px) = scale(viewport_rect.width()) else {
        return;
    };
    let Some(client_height_px) = scale(viewport_rect.height()) else {
        return;
    };
    let Some(left) = scale(rect.left()) else {
        return;
    };
    let Some(top) = scale(rect.top()) else {
        return;
    };
    let Some(right) = scale(rect.right()) else {
        return;
    };
    let Some(bottom) = scale(rect.bottom()) else {
        return;
    };
    if right <= left || bottom <= top {
        return;
    }
    let Some(clip_bounds) = designer_authoring_clip_bounds(ui, viewport_rect.size()) else {
        return;
    };
    acceptance_trace::emit_designer_authoring_control(
        target,
        role,
        viewport,
        index,
        [left, top, right, bottom],
        clip_bounds,
        [client_width_px, client_height_px],
        enabled,
        selected,
        focused,
        clicked,
        correlation.session_id,
        correlation.generation,
        ui.ctx().cumulative_frame_nr(),
        scope,
    );
}

fn designer_authoring_clip_bounds(ui: &egui::Ui, client_size: egui::Vec2) -> Option<[i32; 4]> {
    // Ui owns the ScrollArea clip; viewport.inner_rect itself uses screen coordinates.
    let clip = ui
        .clip_rect()
        .intersect(egui::Rect::from_min_size(egui::Pos2::ZERO, client_size));
    let pixels_per_point = ui.ctx().pixels_per_point();
    if !clip.is_positive() || !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return None;
    }
    let scale = |coordinate: f32| {
        let value = coordinate * pixels_per_point;
        (value.is_finite() && value >= i32::MIN as f32 && value <= i32::MAX as f32)
            .then(|| value.round() as i32)
    };
    Some([
        scale(clip.left())?,
        scale(clip.top())?,
        scale(clip.right())?,
        scale(clip.bottom())?,
    ])
}

fn designer_scroll_viewport_measurement<R>(
    ui: &egui::Ui,
    scroll: &egui::scroll_area::ScrollAreaOutput<R>,
    paint_clip: egui::Rect,
    owner: DesignerAuthoringScrollOwner,
    correlation: Correlation,
) -> Option<DesignerAuthoringScrollViewport> {
    let client_size = ui.ctx().input(|input| input.viewport().inner_rect)?.size();
    let client = egui::Rect::from_min_size(egui::Pos2::ZERO, client_size);
    let pixels_per_point = ui.ctx().pixels_per_point();
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return None;
    }
    let paint_clip = paint_clip.intersect(client);
    // inner_rect is a public, measured subset of egui's wheel-admitting outer
    // region. Content paint clips include clip_rect_margin and are not input
    // viewports. Intersect the real ancestor clip, never infer an inset from it.
    // Egui returns the pre-end inner_rect. Auto-shrink can reduce its final
    // wheel region to content_size; both public measurements are required for
    // a conservative subset, including a short or narrow pane.
    let content_extent = egui::Rect::from_min_size(scroll.inner_rect.min, scroll.content_size);
    let input = scroll
        .inner_rect
        .intersect(content_extent)
        .intersect(ui.clip_rect())
        .intersect(paint_clip);
    let px = |value: f32, inward: bool, minimum: bool| {
        let value = value * pixels_per_point;
        let value = if inward {
            if minimum { value.ceil() } else { value.floor() }
        } else {
            value.round()
        };
        (value.is_finite() && value >= i32::MIN as f32 && value <= i32::MAX as f32)
            .then_some(value as i32)
    };
    let measured = DesignerAuthoringScrollViewport {
        owner,
        scroll_id: scroll.id.value(),
        frame_nr: ui.ctx().cumulative_frame_nr(),
        session_id: correlation.session_id,
        generation: correlation.generation,
        input_bounds: [
            px(input.left(), true, true)?,
            px(input.top(), true, true)?,
            px(input.right(), true, false)?,
            px(input.bottom(), true, false)?,
        ],
        paint_clip_bounds: [
            px(paint_clip.left(), false, true)?,
            px(paint_clip.top(), false, true)?,
            px(paint_clip.right(), false, false)?,
            px(paint_clip.bottom(), false, false)?,
        ],
        client_size: [
            px(client_size.x, false, false)?,
            px(client_size.y, false, false)?,
        ],
    };
    measured.is_valid().then_some(measured)
}

fn show_designer_scroll_area<R>(
    ui: &mut egui::Ui,
    scroll: egui::ScrollArea,
    owner: DesignerAuthoringScrollOwner,
    viewport: ViewportClass,
    correlation: Correlation,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    if !acceptance_trace::enabled() && !cfg!(test) {
        return scroll.show(ui, add_contents);
    }
    let mut paint_clip = egui::Rect::NOTHING;
    let output = scroll.show(ui, |ui| {
        paint_clip = ui.clip_rect();
        add_contents(ui)
    });
    if let Some(measured) =
        designer_scroll_viewport_measurement(ui, &output, paint_clip, owner, correlation)
    {
        acceptance_trace::emit_designer_authoring_scroll_viewport(viewport, measured);
    }
    output
}

fn designer_tree_search_undo_scope(
    context: &egui::Context,
    field_id: egui::Id,
    value: &str,
) -> Option<acceptance_trace::DesignerAuthoringControlScope> {
    // Read the state stored by the actual TextEdit after it has fed this frame.
    // The framework alone owns its undo timing and committed checkpoints.
    let state = egui::text_edit::TextEditState::load(context, field_id)?;
    Some(
        acceptance_trace::DesignerAuthoringControlScope::TreeSearchUndo {
            field_id_digest: field_id.value(),
            value_digest: acceptance_observation_digest(value.as_bytes()),
            in_flux: state.undoer().is_in_flux(),
        },
    )
}

fn designer_rendered_text(
    ui: &egui::Ui,
    response: &egui::Response,
    galley: &egui::Galley,
) -> Option<acceptance_trace::DesignerRenderedText> {
    let viewport = ui.ctx().input(|input| input.viewport().inner_rect)?;
    let text_rect = ui.layout().align_size_within_rect(
        galley.size(),
        response.rect.shrink2(ui.spacing().button_padding),
    );
    let pixels_per_point = ui.ctx().pixels_per_point();
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return None;
    }
    let scale = |coordinate: f32| {
        let value = coordinate * pixels_per_point;
        (value.is_finite() && value >= i32::MIN as f32 && value <= i32::MAX as f32)
            .then(|| value.round() as i32)
    };
    Some(acceptance_trace::DesignerRenderedText {
        digest: acceptance_observation_digest(galley.text().as_bytes()),
        bounds: [
            scale(text_rect.left())?,
            scale(text_rect.top())?,
            scale(text_rect.right())?,
            scale(text_rect.bottom())?,
        ],
        fully_visible: ui.is_rect_visible(response.rect)
            && ui.clip_rect().contains_rect(text_rect)
            && egui::Rect::from_min_size(egui::Pos2::ZERO, viewport.size())
                .contains_rect(text_rect),
        elided: galley.elided,
    })
}

fn trace_inspector_cell_text_edit(
    ui: &egui::Ui,
    response: &egui::Response,
    menu_id: &MenuId,
    ring_id: &RingId,
    cell_id: &CellId,
    session_id: u64,
    generation: u64,
    value: &str,
) {
    if !acceptance_trace::enabled() && !cfg!(test) {
        return;
    }
    let scale = |coordinate: f32| {
        let value = coordinate * ui.ctx().pixels_per_point();
        (value.is_finite() && value >= i32::MIN as f32 && value <= i32::MAX as f32)
            .then(|| value.round() as i32)
    };
    let to_pixels = |rect: egui::Rect| -> Option<[i32; 4]> {
        Some([
            scale(rect.left())?,
            scale(rect.top())?,
            scale(rect.right())?,
            scale(rect.bottom())?,
        ])
    };
    let Some(viewport) = ui.ctx().input(|input| input.viewport().inner_rect) else {
        return;
    };
    let Some(bounds) = to_pixels(response.rect) else {
        return;
    };
    let Some(clipped) = to_pixels(response.rect.intersect(ui.clip_rect())) else {
        return;
    };
    let (Some(client_width), Some(client_height)) =
        (scale(viewport.width()), scale(viewport.height()))
    else {
        return;
    };
    acceptance_trace::emit_inspector_cell_text_edit(
        &menu_id.to_string(),
        &ring_id.to_string(),
        &cell_id.to_string(),
        session_id,
        generation,
        value,
        bounds,
        clipped,
        [client_width, client_height],
        response.has_focus(),
        response.clicked(),
        response.changed(),
    );
}

fn trace_designer_canvas_allocation(
    ui: &egui::Ui,
    response: &egui::Response,
    requested_size: egui::Vec2,
    correlation: Correlation,
) {
    if !acceptance_trace::enabled() {
        return;
    }
    let pixels_per_point = ui.ctx().pixels_per_point();
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return;
    }
    let rect_pixels = |rect: egui::Rect| {
        let px = |coordinate: f32| {
            let coordinate = coordinate * pixels_per_point;
            (coordinate.is_finite()
                && coordinate >= i32::MIN as f32
                && coordinate <= i32::MAX as f32)
                .then(|| coordinate.round() as i32)
        };
        Some([
            px(rect.left())?,
            px(rect.top())?,
            px(rect.right())?,
            px(rect.bottom())?,
        ])
    };
    let Some(allocated_rect_px) = rect_pixels(response.rect) else {
        return;
    };
    let Some(clip_rect_px) = rect_pixels(ui.clip_rect()) else {
        return;
    };
    let requested_width_px = requested_size.x * pixels_per_point;
    let requested_height_px = requested_size.y * pixels_per_point;
    if !requested_width_px.is_finite()
        || !requested_height_px.is_finite()
        || requested_width_px < 0.0
        || requested_height_px < 0.0
        || requested_width_px > i32::MAX as f32
        || requested_height_px > i32::MAX as f32
    {
        return;
    }
    acceptance_trace::emit(Event::DesignerCanvasAllocation {
        allocated_rect_px,
        clip_rect_px,
        requested_size_px: [
            requested_width_px.round() as i32,
            requested_height_px.round() as i32,
        ],
        session_id: correlation.session_id,
        generation: correlation.generation,
    });
}

pub(super) fn menu_cell_ids_digest(menu: &MenuDefinition) -> u64 {
    static HASH_STATE: OnceLock<RandomState> = OnceLock::new();
    let mut hasher = HASH_STATE.get_or_init(RandomState::new).build_hasher();
    menu.id.as_str().hash(&mut hasher);
    menu.rings.len().hash(&mut hasher);
    for ring in &menu.rings {
        ring.id.as_str().hash(&mut hasher);
        ring.cells.len().hash(&mut hasher);
        for cell in &ring.cells {
            cell.id.as_str().hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn stable_cell_id_digest(cell_id: &str) -> u64 {
    static HASH_STATE: OnceLock<RandomState> = OnceLock::new();
    HASH_STATE.get_or_init(RandomState::new).hash_one(cell_id) & i64::MAX as u64
}

fn designer_viewport_builder(open: bool, launcher_always_on_top: bool) -> egui::ViewportBuilder {
    let builder = egui::ViewportBuilder::default()
        .with_title(crate::gui::RADIAL_DESIGNER_WINDOW_TITLE)
        .with_min_inner_size([520.0, 380.0])
        .with_resizable(true)
        .with_visible(open);
    if launcher_always_on_top {
        // The Designer is an independent top-level viewport. Keep it in the
        // same z-order band as ROOT so ROOT's configured topmost state cannot
        // cover the editor's native input surface.
        builder.with_always_on_top()
    } else {
        builder
    }
}

fn mark_trace_submission(previous: &mut Option<(u64, u64)>, current: Option<(u64, u64)>) -> bool {
    if *previous == current {
        return false;
    }
    *previous = current;
    current.is_some()
}

#[derive(Clone, Debug)]
pub(crate) enum DesignerUiIntent {
    ActionSearch {
        identity: AuthoringBindingEditorIdentity,
        query: String,
    },
    TestAction {
        identity: AuthoringBindingEditorIdentity,
        payload: Box<crate::gui::AuthoringProviderTestPayload>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthoringProviderRequestKind {
    Search,
    Test,
}

fn provider_request_kind(
    purpose: &crate::gui::AuthoringProviderSearchPurpose,
) -> AuthoringProviderRequestKind {
    match purpose {
        crate::gui::AuthoringProviderSearchPurpose::Preview => AuthoringProviderRequestKind::Search,
        crate::gui::AuthoringProviderSearchPurpose::Test(_) => AuthoringProviderRequestKind::Test,
    }
}

fn trace_authoring_provider_request_terminal(
    request: &crate::gui::AuthoringProviderSearchRequest,
    edge: &'static str,
) {
    let (kind, binding) = match &request.purpose {
        crate::gui::AuthoringProviderSearchPurpose::Preview => ("search", None),
        crate::gui::AuthoringProviderSearchPurpose::Test(payload) => {
            ("test", Some(&payload.binding))
        }
    };
    acceptance_trace::emit_authoring_provider_search(
        &request.identity,
        edge,
        kind,
        &request.query,
        binding,
        None,
    );
}

#[derive(Clone, Debug)]
enum DesignerFileDialogRequest {
    StyleMedia {
        scope: skin_editor::StyleScope,
        section: String,
        field: String,
        media_kind: crate::radial::model::MediaKind,
        managed: bool,
    },
    BulkStyleMedia {
        request: BulkCellEditRequest,
        field: crate::radial::skin::StyleField,
        media_kind: crate::radial::model::MediaKind,
        managed: bool,
    },
    CellExternalIcon {
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
    },
    ImportManaged {
        kind: crate::radial::model::MediaKind,
        label: String,
        extensions: Vec<String>,
    },
    PreviewPackage,
    ExportMenu {
        menu_id: MenuId,
    },
    ExportPackage {
        roots: Vec<MenuId>,
    },
    ExportSkin {
        skin_id: crate::radial::model::SkinId,
    },
    PreviewLegacy {
        source: crate::radial::compatibility::CompatibilitySource,
    },
    ReplaceBackup,
}

#[derive(Clone, Debug)]
enum DesignerFileDialogResult {
    Cancelled,
    File(std::path::PathBuf),
    Files(Vec<std::path::PathBuf>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BulkStyleMode {
    Inherit,
    Clear,
    #[default]
    Set,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BulkEditStageScope {
    editor_session: crate::radial::authoring::AuthoringSessionId,
    generation: crate::radial::authoring::DraftGeneration,
    targets: Vec<crate::radial::authoring::AuthoredCellTarget>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertiesNavigationDecision {
    Apply,
    Discard,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DesignerSearchHit {
    label: String,
    context: String,
    target: StableSelection,
}

fn authored_search_hits(document: &RadialDocument, query: &str) -> Vec<DesignerSearchHit> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for menu in &document.menus {
        if format!("{} {}", menu.name, menu.id)
            .to_lowercase()
            .contains(&query)
        {
            hits.push(DesignerSearchHit {
                label: menu.name.clone(),
                context: format!("Menu · {}", menu.id),
                target: StableSelection::Menu(menu.id.clone()),
            });
        }
        for ring in &menu.rings {
            for cell in &ring.cells {
                if format!("{} {}", cell.label, cell.id)
                    .to_lowercase()
                    .contains(&query)
                {
                    hits.push(DesignerSearchHit {
                        label: if cell.label.trim().is_empty() {
                            cell.id.to_string()
                        } else {
                            cell.label.clone()
                        },
                        context: format!("{} · Ring {} · Cell {}", menu.name, ring.id, cell.id),
                        target: StableSelection::Cell {
                            menu_id: menu.id.clone(),
                            ring_id: ring.id.clone(),
                            cell_id: cell.id.clone(),
                        },
                    });
                }
            }
        }
    }
    hits
}

#[derive(Clone, Debug)]
struct PendingRadialAdd {
    binding: crate::radial::model::ActionBinding,
    label: String,
    source_query: String,
    trace_identity: RadialInsertionTraceIdentity,
}

#[derive(Clone, Debug)]
struct RadialInsertionTraceContext {
    identity: RadialInsertionTraceIdentity,
    menu_id: MenuId,
    ring_id: RingId,
    cell_id: CellId,
    prior_binding_digest: u64,
}

#[derive(Default)]
pub(crate) struct DesignerIntentBridge {
    intents: Mutex<VecDeque<DesignerUiIntent>>,
    action_search_cancellations: Mutex<
        Vec<(
            AuthoringBindingEditorIdentity,
            AuthoringProviderRequestKind,
            Arc<std::sync::atomic::AtomicBool>,
        )>,
    >,
    pending_authoring_searches:
        Mutex<VecDeque<(crate::gui::AuthoringProviderSearchRequest, Instant)>>,
    /// Native dialogs are requested by the deferred viewport but must be
    /// opened by the root update after the editor lock is released.  Keeping
    /// this queue on the bridge makes the hand-off explicit and lets the
    /// enqueue side wake the root exactly once per accepted request.
    file_dialogs: Mutex<VecDeque<DesignerFileDialogRequest>>,
    file_dialog_pending: AtomicBool,
    /// A child viewport sets this bit once its debounced presentation state
    /// is ready.  The bit is consumed by the root persistence boundary, so a
    /// quiescent/hidden root still receives one explicit wake rather than
    /// relying on a raw-frame poll.
    preferences_ready: AtomicBool,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    viewport_wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl DesignerIntentBridge {
    fn push(&self, intent: DesignerUiIntent) {
        let accepted = if let Ok(mut intents) = self.intents.lock() {
            if intents.len() < 64 {
                intents.push_back(intent);
                true
            } else {
                false
            }
        } else {
            false
        };
        if !accepted {
            return;
        }
        let wake = self
            .wake
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(Arc::clone));
        if let Some(wake) = wake {
            wake();
        }
    }

    fn set_wake(&self, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = wake;
        }
    }

    pub(crate) fn begin_action_search(
        &self,
        identity: AuthoringBindingEditorIdentity,
    ) -> Arc<std::sync::atomic::AtomicBool> {
        self.begin_action_provider_request(identity, AuthoringProviderRequestKind::Search)
    }

    pub(crate) fn begin_action_test(
        &self,
        identity: AuthoringBindingEditorIdentity,
    ) -> Arc<std::sync::atomic::AtomicBool> {
        self.begin_action_provider_request(identity, AuthoringProviderRequestKind::Test)
    }

    fn begin_action_provider_request(
        &self,
        identity: AuthoringBindingEditorIdentity,
        kind: AuthoringProviderRequestKind,
    ) -> Arc<std::sync::atomic::AtomicBool> {
        let cancellation = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Ok(mut active) = self.action_search_cancellations.lock() {
            for (previous, previous_kind, token) in active.iter() {
                if previous.scope == identity.scope && *previous_kind == kind {
                    token.store(true, Ordering::Release);
                }
            }
            active.retain(|(previous, previous_kind, _)| {
                previous.scope != identity.scope || *previous_kind != kind
            });
            active.push((identity, kind, Arc::clone(&cancellation)));
        }
        cancellation
    }

    pub(crate) fn finish_action_search(
        &self,
        identity: &AuthoringBindingEditorIdentity,
        kind: AuthoringProviderRequestKind,
        cancellation: &Arc<std::sync::atomic::AtomicBool>,
    ) {
        if let Ok(mut active) = self.action_search_cancellations.lock() {
            active.retain(|(active_identity, active_kind, active_cancellation)| {
                active_identity != identity
                    || *active_kind != kind
                    || !Arc::ptr_eq(active_cancellation, cancellation)
            });
        }
    }

    pub(crate) fn queue_authoring_search(
        &self,
        request: crate::gui::AuthoringProviderSearchRequest,
        delay: Duration,
    ) -> Result<(), Box<crate::gui::AuthoringProviderSearchRequest>> {
        let kind = provider_request_kind(&request.purpose);
        let mut replaced = None;
        let result = match self.pending_authoring_searches.lock() {
            Ok(mut pending) => {
                if let Some(index) = pending.iter().position(|(queued, _)| {
                    queued.identity.scope == request.identity.scope
                        && provider_request_kind(&queued.purpose) == kind
                }) {
                    replaced = pending.remove(index);
                } else if pending.len() >= 8 {
                    return Err(Box::new(request));
                }
                pending.push_back((request, Instant::now() + delay));
                Ok(())
            }
            Err(_) => Err(Box::new(request)),
        };
        if let Some((replaced, _)) = replaced {
            replaced.cancellation.store(true, Ordering::Release);
            trace_authoring_provider_request_terminal(&replaced, "cancelled");
            self.finish_action_search(
                &replaced.identity,
                provider_request_kind(&replaced.purpose),
                &replaced.cancellation,
            );
        }
        result
    }

    pub(crate) fn take_ready_authoring_search(
        &self,
        now: Instant,
    ) -> (
        Option<crate::gui::AuthoringProviderSearchRequest>,
        Option<Duration>,
    ) {
        let mut stale = Vec::new();
        let result = if let Ok(mut pending) = self.pending_authoring_searches.lock() {
            let mut index = pending.len();
            while index > 0 {
                index -= 1;
                if pending[index].0.cancellation.load(Ordering::Acquire)
                    && let Some((request, _)) = pending.remove(index)
                {
                    stale.push(request);
                }
            }
            let ready_index = pending.iter().position(|(_, ready_at)| *ready_at <= now);
            if let Some(index) = ready_index {
                pending
                    .remove(index)
                    .map(|(request, _)| (Some(request), None))
            } else {
                let next_due = pending
                    .iter()
                    .map(|(_, ready_at)| ready_at.saturating_duration_since(now))
                    .min();
                Some((None, next_due))
            }
        } else {
            None
        };
        for request in stale {
            trace_authoring_provider_request_terminal(&request, "cancelled");
            self.finish_action_search(
                &request.identity,
                provider_request_kind(&request.purpose),
                &request.cancellation,
            );
        }
        result.unwrap_or((None, None))
    }

    fn cancel_action_searches(&self) {
        if let Ok(mut active) = self.action_search_cancellations.lock() {
            for (_, _, cancellation) in active.iter() {
                cancellation.store(true, Ordering::Release);
            }
            active.clear();
        }
    }

    fn set_viewport_wake(&self, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        if let Ok(mut slot) = self.viewport_wake.lock() {
            *slot = wake;
        }
    }

    fn wake_root(&self) {
        let wake = self
            .wake
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(Arc::clone));
        if let Some(wake) = wake {
            wake();
        }
    }

    fn wake_viewport(&self) {
        let wake = self
            .viewport_wake
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(Arc::clone));
        if let Some(wake) = wake {
            wake();
        }
    }

    fn finish_file_dialog(&self) {
        // The completed request changed Designer state, while any queued
        // request still belongs to the root because native dialogs must not
        // run from the deferred viewport callback.
        self.wake_viewport();
        if self.has_pending_file_dialog() {
            self.wake_root();
        }
    }

    fn enqueue_file_dialog(&self, request: DesignerFileDialogRequest) {
        let accepted = if let Ok(mut requests) = self.file_dialogs.lock() {
            // A stalled native dialog must not allow an unbounded queue to
            // grow behind the root update.
            if requests.len() < 8 {
                requests.push_back(request);
                self.file_dialog_pending.store(true, Ordering::Release);
                true
            } else {
                false
            }
        } else {
            false
        };
        if accepted {
            // Deliberately wake only after the queue lock has been released;
            // the root callback may synchronously inspect the bridge.
            self.wake_root();
        }
    }

    fn take_file_dialog(&self) -> Option<DesignerFileDialogRequest> {
        self.file_dialogs.lock().ok().and_then(|mut requests| {
            let request = requests.pop_front();
            if requests.is_empty() {
                // Update the flag while holding the same queue lock as the
                // pop.  An enqueue that races this operation then acquires
                // the lock afterwards and restores the pending bit instead
                // of having its wake silently lost.
                self.file_dialog_pending.store(false, Ordering::Release);
            }
            request
        })
    }

    pub(crate) fn has_pending_file_dialog(&self) -> bool {
        self.file_dialog_pending.load(Ordering::Acquire)
    }

    fn enqueue_preferences_ready(&self) {
        if !self.preferences_ready.swap(true, Ordering::AcqRel) {
            self.wake_root();
        }
    }

    fn take_preferences_ready(&self) -> bool {
        self.preferences_ready.swap(false, Ordering::AcqRel)
    }

    fn clear_preferences_ready(&self) {
        self.preferences_ready.store(false, Ordering::Release);
    }

    #[cfg(test)]
    fn has_pending_preferences(&self) -> bool {
        self.preferences_ready.load(Ordering::Acquire)
    }

    pub(crate) fn drain(&self) -> Vec<DesignerUiIntent> {
        self.intents
            .lock()
            .map(|mut intents| intents.drain(..).collect())
            .unwrap_or_default()
    }

    fn clear(&self) {
        self.cancel_action_searches();
        let pending = self
            .pending_authoring_searches
            .lock()
            .map(|mut pending| {
                pending
                    .drain(..)
                    .map(|(request, _)| request)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for request in pending {
            trace_authoring_provider_request_terminal(&request, "cancelled");
            self.finish_action_search(
                &request.identity,
                provider_request_kind(&request.purpose),
                &request.cancellation,
            );
        }
        if let Ok(mut intents) = self.intents.lock() {
            intents.clear();
        }
        if let Ok(mut requests) = self.file_dialogs.lock() {
            requests.clear();
            self.file_dialog_pending.store(false, Ordering::Release);
        }
        // Preference readiness belongs to the root persistence hand-off.  A
        // close may clear the request queues and unregister callbacks after
        // setting this bit; leave it intact until ROOT consumes the saved
        // presentation snapshot.
    }
}

#[derive(Clone)]
struct DesignerFrameContext {
    feature_defaults: crate::radial::model::RadialFeatureSettings,
    expected_diagnostics: Vec<crate::radial::diagnostics::RadialDiagnostic>,
    action_catalog: Arc<crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot>,
    require_confirm_destructive: bool,
    intent_bridge: Arc<DesignerIntentBridge>,
    root_window_bridge: crate::visibility::RootWindowBridge,
    navigation_instance: Option<u64>,
    jump_mode_enabled: bool,
    application_input_allowed: bool,
}

#[derive(Clone)]
struct RadialJumpPass {
    scope: JumpModeScope,
    visual_order: Arc<AtomicU64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RadialNativeComboAnchor {
    response_id: egui::Id,
    layer_id: egui::LayerId,
    viewport: egui::ViewportId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RadialChildLifetimeKey {
    kind: RadialDesignerChildKind,
    subject: u64,
}

/// Explicit, callback-local navigation context for one Radial UI pass.
///
/// This is passed through the actual render call chain so neither viewport
/// ownership nor combo provenance is hidden in egui's cross-viewport data.
pub(super) struct RadialUiPass {
    jump: Option<RadialJumpPass>,
    combo_anchors: Vec<RadialNativeComboAnchor>,
    owner: NavigationOwner,
    native_ui_allowed: bool,
    commit_allowed: bool,
}

impl Default for RadialUiPass {
    fn default() -> Self {
        Self {
            jump: None,
            combo_anchors: Vec::new(),
            owner: NavigationOwner::Blocked,
            // Direct legacy and unit-test callers use the unscoped path.
            native_ui_allowed: true,
            commit_allowed: true,
        }
    }
}

impl RadialUiPass {
    fn new(
        scope: Option<JumpModeScope>,
        owner: NavigationOwner,
        native_ui_allowed: bool,
        commit_allowed: bool,
    ) -> Self {
        Self {
            jump: scope.map(RadialJumpPass::new),
            combo_anchors: Vec::new(),
            owner,
            native_ui_allowed,
            commit_allowed,
        }
    }

    fn owns_child(
        &self,
        parent_instance: u64,
        child_instance: u64,
        kind: RadialDesignerChildKind,
        subject: u64,
    ) -> bool {
        self.owner
            == (NavigationOwner::RadialDesignerChild {
                parent_instance,
                child_instance,
                kind,
                subject,
            })
    }

    fn child_jump(&self, namespace: &str) -> Option<RadialJumpPass> {
        self.jump.as_ref().map(|pass| pass.child(namespace))
    }

    pub(super) fn register(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: &str,
        label: &'static str,
        kind: TargetKind,
    ) -> Option<FocusDirective> {
        self.jump
            .as_ref()
            .filter(|_| self.native_ui_allowed)
            .and_then(|jump| jump.register(ui, response, identity, label, kind))
    }

    pub(super) fn register_combo_anchor(&mut self, response: &egui::Response) {
        self.record_combo(response);
    }

    pub(super) fn register_combo(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: &str,
        label: &'static str,
    ) {
        self.record_combo(response);
        self.register(ui, response, identity, label, TargetKind::ComboBox);
    }

    pub(super) fn register_combo_option(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        combo_identity: &str,
        option_identity: &str,
        label: &'static str,
    ) {
        if !self.native_ui_allowed {
            return;
        }
        if let Some(jump) = self.jump.as_ref() {
            jump.combo_popup(combo_identity).register(
                ui,
                response,
                option_identity,
                label,
                TargetKind::Selectable,
            );
        }
    }

    pub(super) fn commit_allowed(&self) -> bool {
        self.commit_allowed
    }

    fn record_combo(&mut self, response: &egui::Response) {
        let clipped = response.interact_rect;
        let viewport = response.ctx.viewport_id();
        if self.native_ui_allowed
            && response.enabled()
            && clipped.is_positive()
            && response.ctx.layer_id_at(clipped.center()) == Some(response.layer_id)
        {
            let anchor = RadialNativeComboAnchor {
                response_id: response.id,
                layer_id: response.layer_id,
                viewport,
            };
            if !self.combo_anchors.contains(&anchor) {
                self.combo_anchors.push(anchor);
            }
        }
    }
}

fn register_radial_combo_anchor(
    ui_pass: &mut RadialUiPass,
    response: &egui::Response,
    semantic_identity: &str,
    _label: &'static str,
) {
    ui_pass.record_combo(response);
    debug_assert!(semantic_identity.starts_with("combo/"));
}

fn register_radial_combo_option(
    jump: Option<&RadialJumpPass>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: &str,
    label: &str,
) {
    if let Some(jump) = jump {
        jump.register(ui, response, identity, label, TargetKind::Selectable);
    }
}

fn radial_jump_target_generation(
    viewport_instance: u64,
    editor_session: crate::radial::authoring::AuthoringSessionId,
    draft_generation: crate::radial::authoring::DraftGeneration,
) -> TargetGeneration {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    viewport_instance.hash(&mut hasher);
    editor_session.hash(&mut hasher);
    draft_generation.hash(&mut hasher);
    TargetGeneration(hasher.finish())
}

fn radial_jump_selection_identity(selection: &StableSelection) -> String {
    format!("entity/{:016x}", radial_selection_digest(selection))
}

fn radial_selection_digest(selection: &StableSelection) -> u64 {
    radial_value_digest(selection)
}

fn radial_value_digest(value: &impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn radial_native_selection_id(
    session: &RadialAuthoringSession,
    selection: &StableSelection,
    control: &'static str,
) -> egui::Id {
    egui::Id::new((
        "radial-designer-authored-control",
        session.editor_session.0,
        control,
        radial_selection_digest(selection),
    ))
}

impl RadialJumpPass {
    fn new(scope: JumpModeScope) -> Self {
        Self {
            scope,
            visual_order: Arc::new(AtomicU64::new(0)),
        }
    }

    fn child(&self, namespace: &str) -> Self {
        Self {
            scope: self.scope.child_namespace(namespace),
            visual_order: Arc::clone(&self.visual_order),
        }
    }

    fn combo_popup(&self, anchor: &str) -> Self {
        Self {
            scope: self.scope.combo_popup_scope(anchor),
            visual_order: Arc::clone(&self.visual_order),
        }
    }

    fn register(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: &str,
        label: &str,
        kind: TargetKind,
    ) -> Option<FocusDirective> {
        let mut order = self.visual_order.fetch_add(1, Ordering::Relaxed);
        register_and_apply_target(
            Some(&self.scope),
            ui,
            response,
            identity,
            label,
            kind,
            &mut order,
        )
    }

    fn register_select(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: &str,
        label: &str,
    ) -> bool {
        let target = self.scope.target_id(identity);
        if self.register(ui, response, identity, label, TargetKind::Action)
            != Some(FocusDirective::Select)
        {
            return false;
        }
        response.request_focus();
        self.scope.record_select_focus_owner(&target, response.id);
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingCellDrop {
    source_menu: MenuId,
    source_ring: RingId,
    source_cell: CellId,
    destination_menu: MenuId,
    destination_ring: RingId,
    destination_cell: CellId,
    destination_index: usize,
    generation: crate::radial::authoring::DraftGeneration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScopeChangeAnchor {
    editor_session: crate::radial::authoring::AuthoringSessionId,
    generation: crate::radial::authoring::DraftGeneration,
    selection: Option<StableSelection>,
}

#[derive(Clone, Debug)]
enum ScopeChangeIntent {
    Navigation(DesignerNavigationIntent),
    Operation(DeferredEditorOperation),
}

#[derive(Clone, Debug)]
struct PendingScopeChange {
    anchor: ScopeChangeAnchor,
    intent: ScopeChangeIntent,
}

#[derive(Clone, Debug)]
enum DeferredEditorOperation {
    CreateMenu {
        interaction: crate::radial::model::InteractionMode,
        submenu_presentation: crate::radial::model::SubmenuPresentation,
    },
    ApplyRingProposal {
        proposal: Box<menu::RingEditProposal>,
        nonce: u64,
    },
    CopyCell {
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
    },
    DeleteCell {
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
    },
    CreateSubmenuAndLink {
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
        name: String,
    },
    LinkExistingSubmenu {
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
        linked_menu: MenuId,
    },
    CloneSubmenuAndLink {
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
        linked_menu: MenuId,
    },
    MoveCell {
        pending: PendingCellDrop,
        resolution: menu::CellDropResolution,
    },
    SetCellContent {
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
        content: CellContent,
    },
    Undo,
    Redo,
    ResolveConflict(ConflictResolution),
}

impl ScopeChangeIntent {
    fn apply_and_continue_is_safe(&self) -> bool {
        match self {
            Self::Navigation(_) => true,
            Self::Operation(
                DeferredEditorOperation::SetCellContent { .. }
                | DeferredEditorOperation::CreateSubmenuAndLink { .. }
                | DeferredEditorOperation::LinkExistingSubmenu { .. }
                | DeferredEditorOperation::CloneSubmenuAndLink { .. }
                | DeferredEditorOperation::ApplyRingProposal { .. }
                | DeferredEditorOperation::Undo
                | DeferredEditorOperation::Redo
                | DeferredEditorOperation::ResolveConflict(_),
            ) => false,
            Self::Operation(_) => true,
        }
    }
}

fn queue_scope_change(
    pending: &mut Option<PendingScopeChange>,
    session: &RadialAuthoringSession,
    intent: ScopeChangeIntent,
) {
    if pending.is_some() {
        return;
    }
    *pending = Some(PendingScopeChange {
        anchor: ScopeChangeAnchor {
            editor_session: session.editor_session,
            generation: session.generation,
            selection: session.selection.clone(),
        },
        intent,
    });
}

/// The popup is a multi-frame editor, not a one-frame command.  Keep all
/// editable values together with the generation and target from which they
/// were initialized so a delayed Apply can never merge stale fields into a
/// newer authoring draft.
#[derive(Clone, Debug, PartialEq)]
struct PropertiesDraft {
    target: StableSelection,
    generation: crate::radial::authoring::DraftGeneration,
    label: String,
    content_kind: u8,
    action_binding: Option<crate::radial::model::ActionBinding>,
    action_editor: ActionBindingEditorState,
    dynamic_source: u8,
    submenu_target: Option<MenuId>,
    icon_kind: u8,
    original_content: CellContent,
    original_icon: crate::radial::model::Override<crate::radial::model::MediaReference>,
}

impl PropertiesDraft {
    fn from_cell(
        target: StableSelection,
        generation: crate::radial::authoring::DraftGeneration,
        cell: &crate::radial::model::CellDefinition,
    ) -> Self {
        Self {
            target,
            generation,
            label: cell.label.clone(),
            content_kind: cell_content_kind_key(&cell.content),
            action_binding: match &cell.content {
                CellContent::Action { binding } => Some(binding.clone()),
                _ => None,
            },
            action_editor: ActionBindingEditorState::default(),
            dynamic_source: match &cell.content {
                CellContent::Dynamic { source } => dynamic_source_key(source),
                _ => 0,
            },
            submenu_target: match &cell.content {
                CellContent::Submenu { menu_id } => Some(menu_id.clone()),
                _ => None,
            },
            icon_kind: match &cell.icon {
                crate::radial::model::Override::Inherit => 0,
                crate::radial::model::Override::Clear => 1,
                crate::radial::model::Override::Value(_) => 2,
            },
            original_content: cell.content.clone(),
            original_icon: cell.icon.clone(),
        }
    }

    fn differs_from(&self, cell: &crate::radial::model::CellDefinition) -> bool {
        self.label != cell.label
            || self.content_kind != cell_content_kind_key(&cell.content)
            || self.action_binding
                != match &cell.content {
                    CellContent::Action { binding } => Some(binding.clone()),
                    _ => None,
                }
            || self.dynamic_source
                != match &cell.content {
                    CellContent::Dynamic { source } => dynamic_source_key(source),
                    _ => 0,
                }
            || self.submenu_target
                != match &cell.content {
                    CellContent::Submenu { menu_id } => Some(menu_id.clone()),
                    _ => None,
                }
            || self.icon_kind
                != match &cell.icon {
                    crate::radial::model::Override::Inherit => 0,
                    crate::radial::model::Override::Clear => 1,
                    crate::radial::model::Override::Value(_) => 2,
                }
            || self.action_editor.has_unassigned_text()
    }

    #[cfg(test)]
    fn is_current(
        &self,
        target: &StableSelection,
        generation: crate::radial::authoring::DraftGeneration,
    ) -> bool {
        &self.target == target && self.generation == generation
    }
}

#[derive(Clone, Debug)]
enum EditorCommand {
    MoveCell {
        source_menu: MenuId,
        source_ring: RingId,
        cell: CellId,
        destination_menu: MenuId,
        destination_ring: RingId,
        index: usize,
        expected_generation: crate::radial::authoring::DraftGeneration,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResourceNoticeSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResourceNotice {
    severity: ResourceNoticeSeverity,
    message: String,
}

impl ResourceNotice {
    fn info(message: impl Into<String>) -> Self {
        Self {
            severity: ResourceNoticeSeverity::Info,
            message: message.into(),
        }
    }
    fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: ResourceNoticeSeverity::Warning,
            message: message.into(),
        }
    }
    fn error(message: impl Into<String>) -> Self {
        Self {
            severity: ResourceNoticeSeverity::Error,
            message: message.into(),
        }
    }
}

fn show_resource_notice(ui: &mut egui::Ui, notice: &ResourceNotice) {
    let (tone, label) = match notice.severity {
        ResourceNoticeSeverity::Info => (crate::gui::theme::widgets::StatusTone::Info, "Info"),
        ResourceNoticeSeverity::Warning => {
            (crate::gui::theme::widgets::StatusTone::Warning, "Warning")
        }
        ResourceNoticeSeverity::Error => (crate::gui::theme::widgets::StatusTone::Error, "Error"),
    };
    crate::gui::theme::widgets::semantic_status(ui, tone, label, Some(&notice.message));
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RadialDiagnosticDisplayModel {
    actionable: Vec<crate::radial::diagnostics::RadialDiagnostic>,
    expected_layout: Vec<crate::radial::diagnostics::RadialDiagnostic>,
    omitted: Option<crate::radial::diagnostics::RadialDiagnosticOmission>,
}

fn radial_diagnostic_display_model(
    diagnostics: impl IntoIterator<Item = crate::radial::diagnostics::RadialDiagnostic>,
) -> RadialDiagnosticDisplayModel {
    let diagnostics = crate::radial::diagnostics::bound_diagnostics(
        diagnostics.into_iter().collect(),
        crate::radial::diagnostics::MAX_EXPECTED_LAYOUT_DIAGNOSTICS,
        crate::radial::diagnostics::MAX_RADIAL_DIAGNOSTICS,
    );
    let omitted = diagnostics
        .iter()
        .find_map(|diagnostic| diagnostic.omission());
    let mut model = RadialDiagnosticDisplayModel {
        omitted,
        ..Default::default()
    };
    for diagnostic in diagnostics {
        if diagnostic.omission().is_some() {
            continue;
        }
        if diagnostic.is_expected_layout() {
            model.expected_layout.push(diagnostic);
        } else {
            model.actionable.push(diagnostic);
        }
    }
    model
}

pub(super) fn show_radial_diagnostics<'a>(
    ui: &mut egui::Ui,
    diagnostics: impl IntoIterator<Item = &'a crate::radial::diagnostics::RadialDiagnostic>,
    show_expected_layout: bool,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
) {
    let model = radial_diagnostic_display_model(diagnostics.into_iter().cloned());
    if !model.actionable.is_empty() || model.omitted.is_some_and(|omitted| omitted.actionable > 0) {
        egui::CollapsingHeader::new(format!(
            "Actionable radial diagnostics ({} shown)",
            model.actionable.len()
        ))
        .id_salt(("radial-actionable-diagnostics", &id_salt))
        .default_open(false)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for diagnostic in &model.actionable {
                        let color = match diagnostic.severity {
                            crate::radial::diagnostics::RadialDiagnosticSeverity::Info => {
                                ui.visuals().text_color()
                            }
                            crate::radial::diagnostics::RadialDiagnosticSeverity::Warning => {
                                ui.visuals().warn_fg_color
                            }
                            crate::radial::diagnostics::RadialDiagnosticSeverity::Error => {
                                ui.visuals().error_fg_color
                            }
                        };
                        ui.colored_label(color, &diagnostic.message);
                    }
                });
            if let Some(omitted) = model.omitted.filter(|omitted| omitted.actionable > 0) {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("{} additional diagnostics omitted", omitted.total),
                );
            }
        });
    }

    let expected_omitted = model.omitted.map_or(0, |omitted| omitted.expected_layout);
    if show_expected_layout && (!model.expected_layout.is_empty() || expected_omitted > 0) {
        egui::CollapsingHeader::new(format!(
            "Expected layout diagnostics ({} shown; {} omitted)",
            model.expected_layout.len(),
            expected_omitted,
        ))
        .id_salt(("radial-expected-layout-diagnostics", &id_salt))
        .default_open(false)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for diagnostic in &model.expected_layout {
                        ui.small(diagnostic.message.to_string());
                    }
                });
        });
    }
}

pub(crate) struct RadialEditorState {
    pub(crate) open: bool,
    viewport_close_pending: bool,
    viewport_restore_pending: bool,
    viewport_focus_pending: bool,
    native_jump_mode: JumpModeUi,
    native_jump_frame: Option<u64>,
    native_combo_anchors: Vec<RadialNativeComboAnchor>,
    native_combo_owner: Option<(NavigationOwner, u64)>,
    child_lifetimes: HashMap<RadialChildLifetimeKey, u64>,
    next_child_lifetime: u64,
    session: Option<RadialAuthoringSession>,
    client: Option<AuthoringClient>,
    preview: EmbeddedPreview,
    appearance: appearance::AppearanceGallery,
    close_intent: CloseIntent,
    close_stop_attempted: bool,
    close_prompt: bool,
    delete_message: Option<String>,
    delete_ring_prompt: Option<(MenuId, RingId)>,
    resize_prompt: Option<menu::ResizePlan>,
    ring_proposal: Option<menu::RingEditProposal>,
    ring_proposal_nonce: Option<u64>,
    next_ring_proposal_nonce: u64,
    properties_handoff_prompt: bool,
    pending_scope_change: Option<PendingScopeChange>,
    pending_hotkey_directory_navigation: Option<PendingHotkeyDirectoryNavigation>,
    tree_filter: String,
    bulk_label_value: String,
    bulk_edit_stage_scope: Option<BulkEditStageScope>,
    bulk_style_field: String,
    bulk_style_text: String,
    bulk_style_mode: BulkStyleMode,
    bulk_style_stage_key: String,
    bulk_style_staged_value: Option<serde_json::Value>,
    bulk_after_action_slot: BulkAfterActionSlot,
    bulk_after_action_policy: AfterActionPolicy,
    bulk_feedback: Option<String>,
    submenu_name: String,
    submenu_link_target: Option<MenuId>,
    drag_source: Option<(
        MenuId,
        RingId,
        CellId,
        crate::radial::authoring::DraftGeneration,
    )>,
    post_render: Vec<EditorCommand>,
    preview_zoom: f32,
    preview_preset: PreviewPreset,
    sample_native_context: bool,
    pending_import: Option<PendingImport>,
    export_destination: Option<std::path::PathBuf>,
    replace_backup_path: Option<std::path::PathBuf>,
    replace_confirmed: bool,
    resource_notice: Option<ResourceNotice>,
    show_resources: bool,
    style_text_inputs: std::collections::BTreeMap<String, String>,
    ring_resize_drafts: std::collections::BTreeMap<(MenuId, RingId), usize>,
    focus_restore: Option<StableSelection>,
    preferences: crate::settings::RadialDesignerPreferences,
    preferences_dirty: bool,
    preference_debounce: PreferenceDebounce,
    preferences_flush_requested: bool,
    designer_mode: DesignerMode,
    tree_visible: bool,
    tree_widget_epoch: u64,
    inspector_visible: bool,
    tree_width: f32,
    inspector_width: f32,
    canvas_pan: CanvasPoint,
    pan_drag_start: Option<CanvasPoint>,
    visited_path: VisitedMenuPath,
    projected_selection: Option<ProjectedSelection>,
    drag_payload: Option<DragPayload>,
    placement_draft: Option<PlacementDraft>,
    pending_drop: Option<PendingCellDrop>,
    properties_popup: Option<StableSelection>,
    properties_draft: Option<PropertiesDraft>,
    radial_add_request: Option<PendingRadialAdd>,
    last_radial_add_trace: Option<RadialInsertionTraceContext>,
    last_radial_add_reopen_pending: bool,
    radial_add_menu: Option<MenuId>,
    radial_add_ring: Option<RingId>,
    radial_add_cell: Option<CellId>,
    radial_add_replace_confirmed: bool,
    radial_add_close_tree_confirmed: bool,
    inspector_action_editor: ActionBindingEditorState,
    inspector_action_editor_scope: Option<BindingEditorScope>,
    trace_submission: Option<(u64, u64)>,
    intent_bridge: Arc<DesignerIntentBridge>,
}

impl Default for RadialEditorState {
    fn default() -> Self {
        Self {
            open: false,
            viewport_close_pending: false,
            viewport_restore_pending: true,
            viewport_focus_pending: false,
            native_jump_mode: JumpModeUi::default(),
            native_jump_frame: None,
            native_combo_anchors: Vec::new(),
            native_combo_owner: None,
            child_lifetimes: HashMap::new(),
            next_child_lifetime: 1,
            session: None,
            client: None,
            preview: EmbeddedPreview::default(),
            appearance: appearance::AppearanceGallery::default(),
            close_intent: CloseIntent::None,
            close_stop_attempted: false,
            close_prompt: false,
            delete_message: None,
            delete_ring_prompt: None,
            resize_prompt: None,
            ring_proposal: None,
            ring_proposal_nonce: None,
            next_ring_proposal_nonce: 1,
            properties_handoff_prompt: false,
            pending_scope_change: None,
            pending_hotkey_directory_navigation: None,
            tree_filter: String::new(),
            bulk_label_value: String::new(),
            bulk_edit_stage_scope: None,
            bulk_style_field: "text.bold".into(),
            bulk_style_text: String::new(),
            bulk_style_mode: BulkStyleMode::Set,
            bulk_style_stage_key: String::new(),
            bulk_style_staged_value: None,
            bulk_after_action_slot: BulkAfterActionSlot::Primary,
            bulk_after_action_policy: AfterActionPolicy::Inherit,
            bulk_feedback: None,
            submenu_name: "New submenu".into(),
            submenu_link_target: None,
            drag_source: None,
            post_render: Vec::new(),
            preview_zoom: 1.0,
            preview_preset: PreviewPreset::Current,
            sample_native_context: false,
            pending_import: None,
            export_destination: None,
            replace_backup_path: None,
            replace_confirmed: false,
            resource_notice: None,
            show_resources: false,
            style_text_inputs: Default::default(),
            ring_resize_drafts: Default::default(),
            focus_restore: None,
            preferences: Default::default(),
            preferences_dirty: false,
            preference_debounce: PreferenceDebounce::default(),
            preferences_flush_requested: false,
            designer_mode: DesignerMode::Design,
            tree_visible: false,
            tree_widget_epoch: 0,
            inspector_visible: false,
            tree_width: 180.0,
            inspector_width: 300.0,
            canvas_pan: CanvasPoint::default(),
            pan_drag_start: None,
            visited_path: VisitedMenuPath::default(),
            projected_selection: None,
            drag_payload: None,
            placement_draft: None,
            pending_drop: None,
            properties_popup: None,
            properties_draft: None,
            radial_add_request: None,
            last_radial_add_trace: None,
            last_radial_add_reopen_pending: false,
            radial_add_menu: None,
            radial_add_ring: None,
            radial_add_cell: None,
            radial_add_replace_confirmed: false,
            radial_add_close_tree_confirmed: false,
            inspector_action_editor: ActionBindingEditorState::default(),
            inspector_action_editor_scope: None,
            trace_submission: None,
            intent_bridge: Arc::new(DesignerIntentBridge::default()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DesignerPaneLayout {
    tree_width: f32,
    canvas_width: f32,
    inspector_width: f32,
    height: f32,
}

const DESIGNER_PANE_GAP: f32 = 6.0;

/// Allocate every Designer pane from the actual remaining viewport.  Side
/// panes yield width before the canvas does, and the final widths always fit
/// within the caller's bounded rect (including compact 720px windows). Each
/// explicit horizontal gap follows egui's automatic item spacing. Reserve
/// trailing vertical spacing and a small native-client rounding guard below
/// the row so its response rect stays inside the client at compact heights.
fn designer_pane_layout(
    available: egui::Vec2,
    item_spacing: egui::Vec2,
    tree_visible: bool,
    inspector_visible: bool,
    tree_width: f32,
    inspector_width: f32,
) -> DesignerPaneLayout {
    const MIN_CANVAS: f32 = 140.0;
    const MIN_TREE: f32 = 120.0;
    const MIN_INSPECTOR: f32 = 220.0;
    const CLIENT_EDGE_ROUNDING_GUARD: f32 = 4.0;

    let width = if available.x.is_finite() {
        available.x.clamp(1.0, 10_000.0)
    } else {
        1.0
    };
    let height = if available.y.is_finite() && item_spacing.y.is_finite() {
        (available.y - item_spacing.y.max(0.0) - CLIENT_EDGE_ROUNDING_GUARD).clamp(1.0, 10_000.0)
    } else {
        1.0
    };
    let gap_count = [tree_visible, inspector_visible]
        .into_iter()
        .filter(|visible| *visible)
        .count();
    let horizontal_item_spacing = if item_spacing.x.is_finite() {
        item_spacing.x.max(0.0)
    } else {
        0.0
    };
    let gaps = (DESIGNER_PANE_GAP + horizontal_item_spacing) * gap_count as f32;
    let mut tree = if tree_visible {
        tree_width.clamp(MIN_TREE, 420.0)
    } else {
        0.0
    };
    let mut inspector = if inspector_visible {
        inspector_width.clamp(MIN_INSPECTOR, 560.0)
    } else {
        0.0
    };
    let side_total = tree + inspector;
    let side_budget = (width - gaps - MIN_CANVAS).max(0.0);
    if side_total > side_budget && side_total > 0.0 {
        let ratio = side_budget / side_total;
        tree *= ratio;
        inspector *= ratio;
    }
    let canvas = (width - gaps - tree - inspector).max(1.0);
    DesignerPaneLayout {
        tree_width: tree,
        canvas_width: canvas,
        inspector_width: inspector,
        height,
    }
}

impl RadialEditorState {
    pub(crate) fn set_preferences(
        &mut self,
        preferences: crate::settings::RadialDesignerPreferences,
    ) {
        let previous = preferences.clone().normalized();
        let preferences = preferences.migrate_layout();
        self.designer_mode = match preferences.active_mode {
            crate::settings::RadialDesignerMode::Design => DesignerMode::Design,
            crate::settings::RadialDesignerMode::PreviewTest => DesignerMode::PreviewTest,
        };
        self.tree_visible = preferences.tree_visible;
        self.inspector_visible = preferences.inspector_visible;
        self.show_resources = preferences.show_skins;
        self.tree_width = preferences.tree_width;
        self.inspector_width = preferences.inspector_width;
        self.preview_zoom = preferences.zoom;
        self.canvas_pan = CanvasPoint::new(preferences.pan.0, preferences.pan.1);
        self.preferences = preferences.clone();
        // Persist a one-time legacy presentation upgrade, but do not mark a
        // normal startup dirty.  Only the presentation marker and pane
        // visibility can change here; the authoring draft is untouched.
        self.preferences_dirty = preferences != previous;
        self.preference_debounce.clear();
        if self.preferences_dirty {
            self.preference_debounce.mark_changed(Instant::now());
        }
        self.preferences_flush_requested = false;
        self.intent_bridge.clear_preferences_ready();
    }

    /// Restore only the Designer presentation.  The authoring session,
    /// selection, undo history, assets, and persisted radial document are
    /// intentionally outside this boundary.
    fn reset_designer_layout(&mut self) {
        let preferences = crate::settings::RadialDesignerPreferences::default();
        self.preferences = preferences.clone();
        self.designer_mode = match preferences.active_mode {
            crate::settings::RadialDesignerMode::Design => DesignerMode::Design,
            crate::settings::RadialDesignerMode::PreviewTest => DesignerMode::PreviewTest,
        };
        self.tree_visible = preferences.tree_visible;
        // Reset retained egui collapse state along with the presentation
        // preferences.  Ordinary preference loads must preserve explicit
        // expansion choices, so the widget namespace only changes here.
        self.tree_widget_epoch = self.tree_widget_epoch.wrapping_add(1);
        self.inspector_visible = preferences.inspector_visible;
        self.show_resources = preferences.show_skins;
        self.tree_width = preferences.tree_width;
        self.inspector_width = preferences.inspector_width;
        self.preview_zoom = preferences.zoom;
        self.canvas_pan = CanvasPoint::new(preferences.pan.0, preferences.pan.1);
        self.pan_drag_start = None;
        self.viewport_restore_pending = true;
        self.preview.cancel_tooltip();
        self.mark_preferences_changed();
    }

    fn mark_preferences_changed(&mut self) {
        self.preferences_dirty = true;
        self.preference_debounce.mark_changed(Instant::now());
        self.preferences.layout_version =
            crate::settings::RadialDesignerPreferences::CURRENT_LAYOUT_VERSION;
        self.preferences.tree_visible = self.tree_visible;
        self.preferences.inspector_visible = self.inspector_visible;
        self.preferences.show_skins = self.show_resources;
        self.preferences.tree_width = self.tree_width;
        self.preferences.inspector_width = self.inspector_width;
        self.preferences.zoom = self.preview_zoom;
        self.preferences.pan = (self.canvas_pan.x, self.canvas_pan.y);
        self.preferences.active_mode = match self.designer_mode {
            DesignerMode::Design => crate::settings::RadialDesignerMode::Design,
            DesignerMode::PreviewTest => crate::settings::RadialDesignerMode::PreviewTest,
        };
    }

    fn enqueue_preferences_ready_if_due(&self) {
        if self.preferences_dirty && self.preference_debounce.ready(Instant::now()) {
            self.intent_bridge.enqueue_preferences_ready();
        }
    }

    pub(crate) fn take_preferences_for_persist(
        &mut self,
    ) -> Option<crate::settings::RadialDesignerPreferences> {
        let signaled = self.intent_bridge.take_preferences_ready();
        let ready = signaled || self.preference_debounce.ready(Instant::now());
        (self.preferences_dirty && (ready || self.preferences_flush_requested)).then(|| {
            self.preferences_dirty = false;
            self.preference_debounce.clear();
            self.preferences_flush_requested = false;
            self.preferences.clone().normalized()
        })
    }

    pub(crate) fn intent_bridge(&self) -> Arc<DesignerIntentBridge> {
        Arc::clone(&self.intent_bridge)
    }

    pub(crate) fn acceptance_observation(
        &self,
    ) -> Result<crate::gui::query_observation::AuthoringEditorObservation, String> {
        use std::hash::{Hash, Hasher};

        let session = self.session.as_ref();
        let target = self
            .properties_draft
            .as_ref()
            .map(|draft| &draft.target)
            .or(self.properties_popup.as_ref())
            .or_else(|| session.and_then(|session| session.selection.as_ref()));
        let digest_selection = |selection: Option<&StableSelection>| {
            selection.map_or(0, |selection| {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                selection.hash(&mut hasher);
                hasher.finish()
            })
        };
        let selected_target_digest = digest_selection(target);
        let selected_cell_digest = target.map_or(0, |selection| match selection {
            StableSelection::Cell { cell_id, .. } => {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                cell_id.hash(&mut hasher);
                hasher.finish()
            }
            StableSelection::CellSet(cells) => {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                cells.primary.cell_id.hash(&mut hasher);
                hasher.finish()
            }
            _ => 0,
        });
        let selection = session.and_then(|session| session.selection.as_ref());
        let selected_members = selection
            .map(StableSelection::selected_cells)
            .unwrap_or_default();
        let selected_member_target_digests = selected_members
            .iter()
            .map(acceptance_authored_target_digest)
            .collect::<Vec<_>>();
        let digest_cells = |cells: &[crate::radial::authoring::AuthoredCellTarget]| {
            if cells.is_empty() {
                return 0;
            }
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            cells.hash(&mut hasher);
            hasher.finish()
        };
        let primary_cell_digest = selection
            .and_then(StableSelection::primary_cell)
            .map_or(0, |selection| digest_selection(Some(&selection)));
        let range_anchor_digest = selection.map_or(0, |selection| match selection {
            StableSelection::CellSet(cells) => {
                digest_cells(std::slice::from_ref(&cells.range_anchor))
            }
            _ => digest_cells(&selected_members[..selected_members.len().min(1)]),
        });
        let primary_target_digest = selection.map_or(0, |selection| match selection {
            StableSelection::Cell {
                menu_id,
                ring_id,
                cell_id,
            } => {
                acceptance_identity_digest(&[menu_id.as_str(), ring_id.as_str(), cell_id.as_str()])
            }
            StableSelection::CellSet(cells) => acceptance_authored_target_digest(&cells.primary),
            _ => 0,
        });
        let range_anchor_target_digest = selection.map_or(0, |selection| match selection {
            StableSelection::Cell {
                menu_id,
                ring_id,
                cell_id,
            } => {
                acceptance_identity_digest(&[menu_id.as_str(), ring_id.as_str(), cell_id.as_str()])
            }
            StableSelection::CellSet(cells) => {
                acceptance_authored_target_digest(&cells.range_anchor)
            }
            _ => 0,
        });
        let document_digest = session
            .map(|session| {
                serde_json::to_vec(session.draft.as_ref())
                    .map(|bytes| acceptance_observation_digest(&bytes))
                    .map_err(|error| format!("could not summarize the Designer draft: {error}"))
            })
            .transpose()?
            .unwrap_or(0);
        let pending_assets_digest = session.map_or(0, |session| {
            acceptance_observation_digest(format!("{:?}", session.pending_assets).as_bytes())
        });
        let designer_filter_digest = if self.tree_filter.is_empty() {
            0
        } else {
            acceptance_observation_digest(self.tree_filter.as_bytes())
        };
        let designer_search_hit_count = session.map_or(0, |session| {
            authored_search_hits(&session.draft, &self.tree_filter).len()
        });
        let mut assigned_binding = target.and_then(|selection| {
            let Some(StableSelection::Cell {
                menu_id,
                ring_id,
                cell_id,
            }) = selection.primary_cell()
            else {
                return None;
            };
            let session = session?;
            session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == menu_id)
                .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
                .and_then(|ring| ring.cells.iter().find(|cell| cell.id == cell_id))
                .and_then(|cell| match &cell.content {
                    CellContent::Action { binding } => Some(binding.clone()),
                    _ => None,
                })
        });
        if let Some(draft) = self
            .properties_draft
            .as_ref()
            .filter(|draft| Some(&draft.target) == target)
        {
            assigned_binding.clone_from(&draft.action_binding);
        }
        let binding_digest = |binding: Option<&crate::radial::model::ActionBinding>| {
            binding
                .and_then(|binding| serde_json::to_vec(binding).ok())
                .as_deref()
                .map_or(0, acceptance_observation_digest)
        };
        let active_properties = self.properties_draft.as_ref().filter(|draft| {
            self.open
                && session.is_some_and(|session| {
                    session.editor_session.0 != 0
                        && session.generation == draft.generation
                        && self.properties_popup.as_ref() == Some(&draft.target)
                })
        });
        let properties_staged_digest = active_properties
            .map(|draft| {
                let session_id = session
                    .map(|session| session.editor_session.0)
                    .ok_or_else(|| "active Properties draft has no Designer session".to_string())?;
                acceptance_staged_properties_digest(session_id, selected_target_digest, draft)
            })
            .transpose()?;
        let action_editor = if let Some(session) = session.filter(|_| self.open) {
            if let Some(draft) = active_properties {
                let scope = BindingEditorScope {
                    surface: BindingEditorSurface::Properties,
                    editor_session: session.editor_session,
                    draft_generation: session.generation,
                    target: draft.target.clone(),
                    slot: BindingEditorSlot::CellPrimary,
                };
                draft
                    .action_editor
                    .matches_scope(&scope)
                    .then(|| draft.action_editor.acceptance_observation())
                    .flatten()
            } else if self.inspector_visible {
                let current_scope = session.selection.as_ref().and_then(|selection| {
                    self.inspector_action_editor_scope.as_ref().filter(|scope| {
                        scope.surface == BindingEditorSurface::Inspector
                            && scope.editor_session == session.editor_session
                            && scope.draft_generation == session.generation
                            && &scope.target == selection
                            && scope.slot == BindingEditorSlot::CellPrimary
                    })
                });
                current_scope
                    .filter(|scope| self.inspector_action_editor.matches_scope(scope))
                    .and_then(|_| self.inspector_action_editor.acceptance_observation())
            } else {
                None
            }
        } else {
            None
        };
        let (undo_depth, redo_depth) = session
            .map(RadialAuthoringSession::acceptance_history_depths)
            .unwrap_or_default();
        Ok(crate::gui::query_observation::AuthoringEditorObservation {
            open: self.open,
            session_id: session.map_or(0, |session| session.editor_session.0),
            generation: session.map_or(0, |session| session.generation.0),
            selected_target_digest,
            selected_cell_digest,
            selection_kind: selection.into(),
            selected_member_count: selected_members.len(),
            selected_members_digest: digest_cells(&selected_members),
            selected_member_target_digests,
            primary_cell_digest,
            range_anchor_digest,
            primary_target_digest,
            range_anchor_target_digest,
            navigation_path_digest: self.visited_path.acceptance_digest(),
            navigation_menu_id_digests: self.visited_path.acceptance_menu_id_digests(),
            navigation_edge_digests: self.visited_path.acceptance_edge_digests(),
            pending_assets_digest,
            designer_filter_digest,
            designer_search_hit_count,
            document_digest,
            assigned_binding_digest: binding_digest(assigned_binding.as_ref()),
            properties_staged_digest,
            draft_dirty: session.is_some_and(RadialAuthoringSession::is_dirty),
            properties_popup_open: self.open && self.properties_popup.is_some(),
            properties_dirty: self.properties_popup_dirty(),
            undo_depth,
            redo_depth,
            initial_snapshot_pending: session
                .is_some_and(RadialAuthoringSession::is_initial_snapshot_pending),
            action_editor,
        })
    }

    pub(crate) fn action_editor_request_is_current(
        &self,
        identity: &AuthoringBindingEditorIdentity,
    ) -> bool {
        self.action_editor_scope_is_current(identity)
            && match identity.scope.surface {
                BindingEditorSurface::Properties => self
                    .properties_draft
                    .as_ref()
                    .is_some_and(|draft| draft.action_editor.matches_identity(identity)),
                BindingEditorSurface::Inspector => {
                    self.inspector_action_editor.matches_identity(identity)
                }
            }
    }

    pub(crate) fn action_editor_search_request_is_current(
        &self,
        identity: &AuthoringBindingEditorIdentity,
    ) -> bool {
        self.action_editor_scope_is_current(identity)
            && match identity.scope.surface {
                BindingEditorSurface::Properties => self
                    .properties_draft
                    .as_ref()
                    .is_some_and(|draft| draft.action_editor.matches_search_identity(identity)),
                BindingEditorSurface::Inspector => self
                    .inspector_action_editor
                    .matches_search_identity(identity),
            }
    }

    fn action_editor_scope_is_current(&self, identity: &AuthoringBindingEditorIdentity) -> bool {
        if !self.open {
            return false;
        }
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        if session.is_closed()
            || session.close_requested()
            || session.is_initial_snapshot_pending()
            || session.editor_session != identity.scope.editor_session
            || session.generation != identity.scope.draft_generation
            || !identity.scope.target.exists_in(&session.draft)
        {
            return false;
        }
        match identity.scope.surface {
            BindingEditorSurface::Properties => {
                self.properties_popup.as_ref() == Some(&identity.scope.target)
                    && self.properties_draft.as_ref().is_some_and(|draft| {
                        draft.target == identity.scope.target
                            && draft.generation == identity.scope.draft_generation
                            && draft.content_kind == 1
                    })
            }
            BindingEditorSurface::Inspector => {
                session.selection.as_ref() == Some(&identity.scope.target)
                    && self.inspector_action_editor_scope.as_ref() == Some(&identity.scope)
            }
        }
    }

    pub(crate) fn action_test_request_is_current(
        &self,
        identity: &AuthoringBindingEditorIdentity,
        binding: &crate::radial::model::ActionBinding,
    ) -> bool {
        if !self.action_editor_request_is_current(identity) {
            return false;
        }
        match identity.scope.surface {
            BindingEditorSurface::Properties => self
                .properties_draft
                .as_ref()
                .is_some_and(|draft| draft.action_editor.test_binding_matches(identity, binding)),
            BindingEditorSurface::Inspector => self
                .inspector_action_editor
                .test_binding_matches(identity, binding),
        }
    }

    pub(crate) fn complete_action_editor_search(
        &mut self,
        completion: ActionSearchCompletion,
    ) -> bool {
        if !self.action_editor_search_request_is_current(&completion.identity) {
            return false;
        }
        match completion.identity.scope.surface {
            BindingEditorSurface::Properties => self
                .properties_draft
                .as_mut()
                .is_some_and(|draft| draft.action_editor.complete_search(completion)),
            BindingEditorSurface::Inspector => {
                self.inspector_action_editor.complete_search(completion)
            }
        }
    }

    pub(crate) fn finish_action_editor_test(
        &mut self,
        identity: &AuthoringBindingEditorIdentity,
        binding: &crate::radial::model::ActionBinding,
        result: Result<(), String>,
    ) -> bool {
        if !self.action_editor_request_is_current(identity) {
            return false;
        }
        match identity.scope.surface {
            BindingEditorSurface::Properties => self
                .properties_draft
                .as_mut()
                .is_some_and(|draft| draft.action_editor.finish_test(identity, binding, result)),
            BindingEditorSurface::Inspector => self
                .inspector_action_editor
                .finish_test(identity, binding, result),
        }
    }

    pub(crate) fn await_action_editor_confirmation(
        &mut self,
        identity: &AuthoringBindingEditorIdentity,
        binding: &crate::radial::model::ActionBinding,
    ) -> bool {
        if !self.action_editor_request_is_current(identity) {
            return false;
        }
        match identity.scope.surface {
            BindingEditorSurface::Properties => {
                self.properties_draft.as_mut().is_some_and(|draft| {
                    draft
                        .action_editor
                        .await_test_confirmation(identity, binding)
                })
            }
            BindingEditorSurface::Inspector => self
                .inspector_action_editor
                .await_test_confirmation(identity, binding),
        }
    }

    /// Drain at most one native file dialog request.  The request is removed
    /// under the editor lock, the blocking native dialog is opened without
    /// that lock, and only then is the result applied to the session.  This is
    /// the ownership boundary that keeps import/export dialogs independent of
    /// the deferred viewport callback.
    pub(crate) fn process_pending_file_dialog(shared: &Arc<Mutex<Self>>) {
        let bridge = shared
            .lock()
            .ok()
            .map(|editor| Arc::clone(&editor.intent_bridge));
        let Some(bridge) = bridge else {
            return;
        };
        let request = bridge.take_file_dialog();
        let Some(request) = request else {
            return;
        };
        let result = open_designer_file_dialog(&request);
        if let Ok(mut editor) = shared.lock() {
            editor.apply_file_dialog_result(request, result);
        }
        // Dialog completion is an event for the independent viewport.  Wake
        // it only after the editor lock is released.
        bridge.finish_file_dialog();
    }

    fn apply_file_dialog_result(
        &mut self,
        request: DesignerFileDialogRequest,
        result: DesignerFileDialogResult,
    ) {
        let (file, files) = match result {
            DesignerFileDialogResult::Cancelled => (None, None),
            DesignerFileDialogResult::File(path) => (Some(path), None),
            DesignerFileDialogResult::Files(paths) => (None, Some(paths)),
        };
        match request {
            DesignerFileDialogRequest::StyleMedia {
                scope,
                section,
                field,
                media_kind,
                managed,
            } => {
                let Some(path) = file else { return };
                let selected =
                    asset_picker::read_bounded(&path, crate::radial::assets::MAX_SOURCE_BYTES)
                        .and_then(|bytes| {
                            asset_picker::ResourceChoice::from_selected_file(
                                &path, bytes, media_kind, managed,
                            )
                        });
                let Some(session) = self.session.as_mut() else {
                    return;
                };
                match selected {
                    Ok(choice) => {
                        self.resource_notice = Some(if managed {
                            ResourceNotice::info(choice.portability_diagnostic())
                        } else {
                            ResourceNotice::warning(choice.portability_diagnostic())
                        });
                        if let Err(error) = skin_editor::set_media_override(
                            session, &scope, &section, &field, &choice,
                        ) {
                            self.resource_notice = Some(ResourceNotice::error(error));
                        }
                    }
                    Err(error) => {
                        self.resource_notice = Some(ResourceNotice::error(error));
                    }
                }
            }
            DesignerFileDialogRequest::BulkStyleMedia {
                request,
                field,
                media_kind,
                managed,
            } => {
                let Some(path) = file else { return };
                let selected =
                    asset_picker::read_bounded(&path, crate::radial::assets::MAX_SOURCE_BYTES)
                        .and_then(|bytes| {
                            asset_picker::ResourceChoice::from_selected_file(
                                &path, bytes, media_kind, managed,
                            )
                        });
                let Some(session) = self.session.as_mut() else {
                    return;
                };
                let result = selected.and_then(|choice| {
                    let value = serde_json::to_value(crate::radial::model::Override::Value(
                        choice.media_reference(),
                    ))
                    .map_err(|error| error.to_string())?;
                    let mut assets = session.pending_assets.clone();
                    choice.stage(&mut assets);
                    let asset_records = match &choice {
                        asset_picker::ResourceChoice::Managed { record, .. } => {
                            vec![record.clone()]
                        }
                        _ => Vec::new(),
                    };
                    menu::apply_bulk_cell_edit_with_assets(
                        session,
                        &request,
                        BulkCellEdit::SetStyleOverride { field, value },
                        assets,
                        asset_records,
                    )
                    .map_err(|error| format!("{error:?}"))
                });
                self.bulk_feedback = Some(match result {
                    Ok(()) => format!(
                        "Applied one atomic media override to {} cells.",
                        request.targets.len()
                    ),
                    Err(error) => format!("Bulk media edit rejected: {error}"),
                });
            }
            DesignerFileDialogRequest::CellExternalIcon {
                menu_id,
                ring_id,
                cell_id,
            } => {
                let Some(path) = file else { return };
                let Some(session) = self.session.as_mut() else {
                    return;
                };
                let mut document = (*session.draft).clone();
                let Some(cell) = document
                    .menus
                    .iter_mut()
                    .find(|menu| menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter_mut().find(|ring| ring.id == ring_id))
                    .and_then(|ring| ring.cells.iter_mut().find(|cell| cell.id == cell_id))
                else {
                    return;
                };
                cell.icon = crate::radial::model::Override::Value(
                    crate::radial::model::MediaReference::ExternalFile {
                        path: path.display().to_string(),
                    },
                );
                if let Err(error) = session.replace_document_atomic(document) {
                    self.resource_notice = Some(ResourceNotice::error(format!("{error:?}")));
                }
            }
            DesignerFileDialogRequest::ImportManaged {
                kind,
                label,
                extensions: _,
            } => {
                let Some(path) = file else { return };
                let selected =
                    asset_picker::read_bounded(&path, crate::radial::assets::MAX_SOURCE_BYTES)
                        .and_then(|bytes| {
                            asset_picker::ResourceChoice::from_selected_file(
                                &path, bytes, kind, true,
                            )
                        })
                        .and_then(|choice| {
                            self.session
                                .as_mut()
                                .ok_or_else(|| "Radial authoring session unavailable".to_owned())
                                .and_then(|session| {
                                    asset_picker::add_resource(session, &choice).map(|_| choice)
                                })
                        });
                match selected {
                    Ok(choice) => {
                        self.resource_notice = Some(ResourceNotice::info(format!(
                            "{}: {}",
                            label,
                            choice.portability_diagnostic()
                        )));
                    }
                    Err(error) => {
                        self.resource_notice = Some(ResourceNotice::error(error));
                    }
                }
            }
            DesignerFileDialogRequest::PreviewPackage => {
                let Some(path) = file else { return };
                let Some(session) = self.session.as_mut() else {
                    return;
                };
                self.pending_import = match asset_picker::read_bounded(
                    &path,
                    crate::radial::package::MAX_PACKAGE_COMPRESSED_BYTES,
                ) {
                    Ok(bytes) => match PendingImport::package(&bytes, session) {
                        Ok(preview) => Some(preview),
                        Err(error) => {
                            self.resource_notice = Some(ResourceNotice::error(error.to_string()));
                            None
                        }
                    },
                    Err(error) => {
                        self.resource_notice = Some(ResourceNotice::error(error.to_string()));
                        None
                    }
                };
            }
            DesignerFileDialogRequest::ExportMenu { menu_id } => {
                let Some(path) = file else { return };
                self.begin_export_package(vec![menu_id], path);
            }
            DesignerFileDialogRequest::ExportPackage { roots } => {
                let Some(path) = file else { return };
                self.begin_export_package(roots, path);
            }
            DesignerFileDialogRequest::ExportSkin { skin_id } => {
                let Some(path) = file else { return };
                let Some(session) = self.session.as_mut() else {
                    return;
                };
                if session.is_dirty() {
                    self.resource_notice = Some(ResourceNotice::warning(
                        "Save or apply the draft before exporting persisted package bytes",
                    ));
                    return;
                }
                match session.request_export_skin(skin_id) {
                    Ok(request) => {
                        let pending = session.pending_request;
                        let result = self.client.as_ref().map_or_else(
                            || Err(AuthoringError::ServiceClosed),
                            |client| client.send(request),
                        );
                        match result {
                            Ok(()) => self.export_destination = Some(path),
                            Err(error) => {
                                if let Some(pending) = pending {
                                    session.reconcile_request_delivery_failure(
                                        pending,
                                        format!("{error:?}"),
                                    );
                                }
                            }
                        }
                    }
                    Err(error) => session.last_error = Some(format!("{error:?}")),
                }
            }
            DesignerFileDialogRequest::PreviewLegacy { source } => {
                let Some(paths) = files else {
                    return;
                };
                let Some(session) = self.session.as_ref() else {
                    return;
                };
                match preview_legacy_files(&paths, source, session) {
                    Ok(preview) => self.pending_import = Some(preview),
                    Err(error) => self.resource_notice = Some(ResourceNotice::error(error)),
                }
            }
            DesignerFileDialogRequest::ReplaceBackup => {
                let Some(path) = file else { return };
                self.replace_backup_path = Some(path);
            }
        }
    }

    fn begin_export_package(&mut self, roots: Vec<MenuId>, path: std::path::PathBuf) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.is_dirty() {
            self.resource_notice = Some(ResourceNotice::warning(
                "Save or apply the draft before exporting persisted package bytes",
            ));
            return;
        }
        match session.request_export_package(roots) {
            Ok(request) => {
                let pending = session.pending_request;
                let result = self.client.as_ref().map_or_else(
                    || Err(AuthoringError::ServiceClosed),
                    |client| client.send(request),
                );
                match result {
                    Ok(()) => self.export_destination = Some(path),
                    Err(error) => {
                        if let Some(pending) = pending {
                            session
                                .reconcile_request_delivery_failure(pending, format!("{error:?}"));
                        }
                    }
                }
            }
            Err(error) => session.last_error = Some(format!("{error:?}")),
        }
    }

    /// Register one stable deferred viewport.  The callback owns only the
    /// editor state and immutable service/catalog snapshots; it never borrows
    /// `LauncherApp` or the root egui frame.
    pub(crate) fn show_deferred(
        shared: &Arc<Mutex<Self>>,
        ctx: &egui::Context,
        app: &LauncherApp,
        navigation_instance: Option<u64>,
        jump_mode_enabled: bool,
        application_input_allowed: bool,
    ) {
        let (
            open,
            viewport_restore_pending,
            viewport_focus_pending,
            preferences,
            feature_defaults,
            diagnostics,
            require_confirm,
            launcher_always_on_top,
        ) = match shared.lock() {
            Ok(editor) => {
                if !editor.open && !editor.viewport_close_pending {
                    return;
                }
                (
                    editor.open,
                    editor.viewport_restore_pending,
                    editor.viewport_focus_pending,
                    editor.preferences.clone().normalized(),
                    app.radial_feature_settings.clone(),
                    app.radial_expected_diagnostics.iter().cloned().collect(),
                    app.require_confirm_destructive,
                    app.always_on_top,
                )
            }
            Err(_) => return,
        };
        let action_catalog = if open {
            app.cached_universal_action_catalog_snapshot()
        } else {
            Arc::new(crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot::empty())
        };
        let viewport_id = radial_designer_viewport_id();
        let reply_ctx = ctx.clone();
        let reply_wake: Arc<dyn Fn() + Send + Sync> =
            Arc::new(move || reply_ctx.request_repaint_of(viewport_id));
        let intent_bridge = shared
            .lock()
            .map(|editor| Arc::clone(&editor.intent_bridge))
            .unwrap_or_else(|_| Arc::new(DesignerIntentBridge::default()));
        let root_ctx = ctx.clone();
        intent_bridge.set_wake(Some(Arc::new(move || {
            root_ctx.request_repaint_of(egui::ViewportId::ROOT);
        })));
        intent_bridge.set_viewport_wake(Some(Arc::clone(&reply_wake)));
        if let Ok(editor) = shared.lock()
            && let Some(client) = editor.client.as_ref()
        {
            client.set_reply_wake(Some(reply_wake));
        }
        let frame = DesignerFrameContext {
            feature_defaults,
            expected_diagnostics: diagnostics,
            action_catalog,
            require_confirm_destructive: require_confirm,
            intent_bridge,
            root_window_bridge: app.root_window_bridge.clone(),
            navigation_instance,
            jump_mode_enabled,
            application_input_allowed,
        };
        let saved_geometry = WindowGeometry {
            position: preferences
                .window_position
                .map(|(x, y)| CanvasPoint::new(x, y)),
            size: CanvasPoint::new(preferences.window_size.0, preferences.window_size.1),
        };
        let saved_position_scale_factor = preferences.window_scale_factor;
        let mut builder = designer_viewport_builder(open, launcher_always_on_top);
        if viewport_restore_pending {
            // The initial builder only supplies a bounded size.  Desktop
            // position and mixed-DPI normalization belong to the child
            // callback, where the Designer's own zoom factor is known.
            builder = builder.with_inner_size([saved_geometry.size.x, saved_geometry.size.y]);
        }
        let shared = Arc::clone(shared);
        ctx.show_viewport_deferred(viewport_id, builder, move |child, class| {
            let trace_viewport = trace_viewport_class(class);
            let (pointer_down, pointer_up) =
                child.input(|input| (input.pointer.any_pressed(), input.pointer.any_released()));
            let Ok(mut editor) = shared.lock() else {
                return;
            };
            let correlation = trace_correlation(editor.session.as_ref());
            let close_requested = class != egui::ViewportClass::EmbeddedWindow
                && child.input(|input| input.viewport().close_requested());
            // Retained deferred callbacks can execute every frame.  Trace
            // only interaction/focus/close edges so the bounded diagnostic
            // budget remains useful for the complete manual sequence.
            let _callback_trace =
                (pointer_down || pointer_up || close_requested || editor.viewport_focus_pending)
                    .then(|| acceptance_trace::DesignerCallbackGuard::enter(trace_viewport));
            if close_requested {
                editor.request_close();
                // A dirty designer must keep its deferred viewport alive long
                // enough to show Save/Discard/Keep editing.  Clean close is
                // handled below by sending the actual Close command.
                if editor.open {
                    child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
            }
            if !editor.open {
                editor.native_jump_mode.cancel_all();
                editor.native_jump_frame = None;
                frame.root_window_bridge.clear_designer_identity();
                // The deferred viewport is removed when ROOT stops registering it.
                // Sending Close here would only enqueue another close-request event
                // in eframe 0.27, which keeps this callback alive and repainting.
                editor.viewport_close_pending = false;
                return;
            }
            if class == egui::ViewportClass::EmbeddedWindow {
                editor.native_jump_mode.cancel_all();
                editor.native_jump_frame = None;
                // Embedded callbacks are not an authoring surface.  Keep the
                // state alive for the caller to close/reopen, but never render
                // the authoritative editor into the root viewport.
                egui::Window::new("Radial Designer unavailable")
                    .collapsible(false)
                    .resizable(false)
                    .default_size(egui::vec2(380.0, 150.0))
                    .show(child, |ui| {
                        ui.label("Radial Designer requires an independent viewport.");
                        ui.small("This host only provides an embedded viewport.");
                    });
                return;
            }
            if child
                .input(|input| input.viewport().focused)
                .unwrap_or(false)
            {
                frame.root_window_bridge.capture_focused_designer();
            }
            let focus_requested = std::mem::take(&mut editor.viewport_focus_pending);
            if focus_requested {
                acceptance_trace::emit(Event::DesignerFocus {
                    edge: FocusEdge::Requested,
                    viewport: trace_viewport,
                    correlation,
                });
            }
            if editor.viewport_restore_pending {
                let restored = work_area::restore_geometry(
                    child,
                    saved_geometry,
                    saved_position_scale_factor,
                    child.zoom_factor(),
                );
                child.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    restored.size.x,
                    restored.size.y,
                )));
                if let Some(position) = restored.position {
                    child.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                        position.x, position.y,
                    )));
                }
                editor.viewport_restore_pending = false;
            }
            if focus_requested {
                acceptance_trace::emit(Event::DesignerFocus {
                    edge: FocusEdge::Consumed,
                    viewport: trace_viewport,
                    correlation,
                });
            }
            editor.viewport_ui(child, &frame, class);
        });
        if viewport_focus_pending {
            // The root command can queue focus after the child last rendered.
            // Register the child first, then send Focus through its parent so
            // eframe wakes and focuses a deferred viewport whose callback is
            // currently idle.  The callback records consumption when it runs.
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
        }
    }

    pub(crate) fn open(&mut self) {
        if self.open {
            self.viewport_focus_pending = true;
            return;
        }
        self.open = true;
        self.native_jump_mode.cancel_all();
        self.native_jump_frame = None;
        self.native_combo_anchors.clear();
        self.native_combo_owner = None;
        self.child_lifetimes.clear();
        self.last_radial_add_reopen_pending = self.last_radial_add_trace.is_some();
        self.viewport_close_pending = false;
        self.viewport_restore_pending = true;
        self.viewport_focus_pending = true;
        self.close_intent = CloseIntent::None;
        self.close_stop_attempted = false;
        self.close_prompt = false;
        self.drag_source = None;
        self.post_render.clear();
        self.focus_restore = None;
        self.visited_path = VisitedMenuPath::new(RadialDocument::starter().default_menu_id);
        self.projected_selection = None;
        self.drag_payload = None;
        self.placement_draft = None;
        self.pending_drop = None;
        self.properties_popup = None;
        self.properties_draft = None;
        self.pending_scope_change = None;
        self.radial_add_request = None;
        self.radial_add_menu = None;
        self.radial_add_ring = None;
        self.radial_add_cell = None;
        self.radial_add_replace_confirmed = false;
        self.radial_add_close_tree_confirmed = false;
        self.properties_handoff_prompt = false;
        self.ring_proposal = None;
        self.ring_proposal_nonce = None;
        self.trace_submission = None;
        self.pan_drag_start = None;
        let document = RadialDocument::starter();
        let mut session = RadialAuthoringSession::new(AuthoringSnapshot {
            revision: document.revision,
            document: std::sync::Arc::new(document),
            disk_sha256: DiskSha256(String::new()),
        });
        session.require_authoritative_snapshot();
        self.client = super::radial_authoring_client();
        if let Some(client) = &self.client {
            client.acquire_resources(session.editor_session());
            if let Ok(request) = session.request_snapshot() {
                let pending = session.pending_request;
                if let Err(error) = client.send(request)
                    && let Some(pending) = pending
                {
                    session.reconcile_request_delivery_failure(pending, format!("{error:?}"));
                }
            }
        }
        self.session = Some(session);
    }

    pub(crate) fn add_action_to_radial(
        &mut self,
        binding: crate::radial::model::ActionBinding,
        label: String,
        source_query: String,
        trace_identity: RadialInsertionTraceIdentity,
    ) {
        self.open();
        self.last_radial_add_trace = None;
        self.last_radial_add_reopen_pending = false;
        self.radial_add_request = Some(PendingRadialAdd {
            binding,
            label,
            source_query,
            trace_identity,
        });
        self.radial_add_menu = self
            .session
            .as_ref()
            .and_then(selected_menu_id)
            .or_else(|| {
                self.session
                    .as_ref()
                    .map(|session| session.draft.default_menu_id.clone())
            });
        self.radial_add_ring = None;
        self.radial_add_cell = None;
        self.radial_add_replace_confirmed = false;
        self.radial_add_close_tree_confirmed = false;
        self.viewport_focus_pending = true;
    }

    /// Keep a pinned Designer open without treating each maintenance pass as
    /// an explicit user focus request.
    pub(crate) fn invalidate_appearance_resources(&mut self) {
        self.appearance.invalidate_resources();
        self.preview.invalidate_resources();
    }

    pub(crate) fn ensure_open(&mut self) {
        if !self.open {
            self.open();
        }
    }

    pub(crate) fn open_skins(&mut self) {
        self.open();
        self.show_resources = true;
        self.mark_preferences_changed();
        acceptance_trace::emit(Event::RadialAction {
            stage: crate::radial::acceptance_trace::RadialActionStage::EditorModeApplied,
            skins: true,
            editor_open: Some(self.open),
            skins_selected: Some(self.show_resources),
            panel_registered: None,
        });
    }

    pub(crate) fn open_menus(&mut self) {
        self.open();
        self.show_resources = false;
        self.mark_preferences_changed();
        acceptance_trace::emit(Event::RadialAction {
            stage: crate::radial::acceptance_trace::RadialActionStage::EditorModeApplied,
            skins: false,
            editor_open: Some(self.open),
            skins_selected: Some(self.show_resources),
            panel_registered: None,
        });
    }

    pub(crate) fn request_hotkey_directory_navigation(
        &mut self,
        target: HotkeyDirectoryRadialTarget,
    ) -> bool {
        if !self.open || self.close_intent != CloseIntent::None {
            return false;
        }
        self.pending_hotkey_directory_navigation = Some(PendingHotkeyDirectoryNavigation {
            target,
            frames_remaining: 8,
        });
        self.apply_pending_hotkey_directory_navigation();
        true
    }

    #[cfg(test)]
    pub(crate) fn is_showing_resources(&self) -> bool {
        self.show_resources
    }

    #[cfg(test)]
    pub(crate) fn is_dirty(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(RadialAuthoringSession::is_dirty)
    }

    #[cfg(test)]
    pub(crate) fn has_close_prompt(&self) -> bool {
        self.close_prompt
    }

    #[cfg(test)]
    pub(crate) fn make_dirty_for_test(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let id = session.draft.menus[0].id.clone();
        session
            .mutate(
                crate::radial::authoring::DocumentMutation::RenameMenu {
                    id,
                    name: "Unsaved test menu".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
    }

    #[cfg(test)]
    pub(crate) fn open_test_snapshot(&mut self) {
        let document = RadialDocument::starter();
        self.open = true;
        self.close_intent = CloseIntent::None;
        self.close_stop_attempted = false;
        self.client = None;
        self.trace_submission = None;
        self.properties_popup = None;
        self.properties_draft = None;
        self.radial_add_close_tree_confirmed = false;
        self.session = Some(RadialAuthoringSession::new(AuthoringSnapshot::new(
            std::sync::Arc::new(document),
            "test",
        )));
    }

    pub(crate) fn request_close(&mut self) {
        self.appearance.dispose();
        if self.close_intent == CloseIntent::Requested {
            return;
        }
        self.close_intent = CloseIntent::Requested;
        self.close_stop_attempted = false;
        if let Some(session) = self.session.as_mut() {
            session.request_close_intent();
        }
        self.preview.cancel_tooltip();
        // Window move/resize state must be persisted when close interaction
        // ends, even if the debounce interval has not elapsed.
        self.preferences_flush_requested = true;
        self.preference_debounce.flush();
        self.intent_bridge.enqueue_preferences_ready();
        if let Some(draft) = self.properties_draft.as_mut() {
            draft.action_editor.invalidate_pending_requests();
        }
        self.inspector_action_editor.invalidate_pending_requests();
        self.intent_bridge.clear();
        self.placement_draft = None;
        self.pending_drop = None;
        // Popup edits are intentionally retained until the normal close
        // decision resolves. They are not yet part of the document dirty bit,
        // but closing the native viewport must not silently discard them.
        // A preview preparation has no durable side effect and can be
        // cancelled locally.  This lets an OS close finish promptly without
        // leaving a late preparation reply attached to a hidden viewport.
        // Read-only/preview preparation can be invalidated locally.  Durable
        // requests remain pending until their correlated terminal reply.
        if let Some(pending) = self
            .session
            .as_ref()
            .and_then(|session| session.pending_request)
            .filter(|pending| pending.kind.is_disposable())
            && let Some(session) = self.session.as_mut()
        {
            acceptance_trace::emit(Event::DisposableRequestCancelled {
                request_kind: trace_request_kind(pending.kind),
                request_id: pending.id.0,
                session_id: pending.editor_session.0,
                generation: pending.generation.0,
            });
            session.cancel_pending_request_exact(pending);
        }
        if let Some(pending) = self
            .session
            .as_ref()
            .and_then(|session| session.pending_native_preview)
            && matches!(
                pending.kind,
                PendingRequestKind::StartNativePreview | PendingRequestKind::UpdateNativePreview
            )
        {
            // The request has already crossed the client boundary.  Queue a
            // correlated stop behind it instead of dropping the local slot:
            // the service will process Start/Update then Stop in order, and
            // the close path will wait for the terminal stop reply.
            self.stop_native_preview();
        }
        self.maybe_finish_close();
        self.trace_close_state();
    }

    fn trace_close_state(&self) {
        let state = self.session.as_ref().map_or_else(
            || DesignerCloseState {
                session_id: 0,
                open: self.open,
                close_prompt: self.close_prompt,
                dirty: self.properties_popup_dirty(),
                pending_disposable: false,
                pending_durable: false,
                pending_native_preview: false,
            },
            |session| DesignerCloseState {
                session_id: session.editor_session.0,
                open: self.open,
                close_prompt: self.close_prompt,
                dirty: session.is_dirty() || self.properties_popup_dirty(),
                pending_disposable: session
                    .pending_request
                    .is_some_and(|pending| pending.kind.is_disposable()),
                pending_durable: session
                    .pending_request
                    .is_some_and(|pending| !pending.kind.is_disposable()),
                pending_native_preview: session.pending_native_preview.is_some(),
            },
        );
        acceptance_trace::emit(Event::DesignerClose { state });
    }

    fn finish_close(&mut self) {
        self.appearance.dispose();
        self.preview.dispose();
        self.release_authoring_resources();
        self.inspector_action_editor.dispose();
        self.inspector_action_editor_scope = None;
        self.properties_popup = None;
        self.properties_draft = None;
        self.open = false;
        self.native_jump_mode.cancel_all();
        self.native_jump_frame = None;
        self.native_combo_anchors.clear();
        self.native_combo_owner = None;
        self.child_lifetimes.clear();
        self.viewport_close_pending = true;
        self.session = None;
        self.trace_submission = None;
        self.close_prompt = false;
        self.close_intent = CloseIntent::None;
        self.close_stop_attempted = false;
        self.radial_add_request = None;
        self.radial_add_menu = None;
        self.radial_add_ring = None;
        self.radial_add_cell = None;
        self.radial_add_replace_confirmed = false;
        self.radial_add_close_tree_confirmed = false;
    }

    fn maybe_finish_close(&mut self) {
        if self.close_intent != CloseIntent::Requested
            && self.close_intent != CloseIntent::DiscardRequested
        {
            return;
        }
        let Some(session) = self.session.as_ref() else {
            self.open = false;
            self.viewport_close_pending = true;
            self.close_intent = CloseIntent::None;
            self.close_stop_attempted = false;
            return;
        };
        if session.is_closed() {
            self.finish_close();
            return;
        }
        // Commit/ReplacePackage and an in-flight native stop are terminal
        // work.  Do not dispose the callbacks or session before their exact
        // reply arrives.
        if session.pending_request.is_some() || session.pending_native_preview.is_some() {
            return;
        }
        let native_preview_active =
            session.native_preview_may_be_open || session.native_preview_lease.is_some();
        if native_preview_active && !self.close_stop_attempted {
            self.stop_native_preview();
            return;
        }
        if self.close_intent == CloseIntent::DiscardRequested {
            self.finish_close();
        } else if session.is_dirty() || self.properties_popup_dirty() {
            self.close_prompt = true;
        } else {
            self.finish_close();
        }
    }

    pub(crate) fn force_close(&mut self) {
        self.appearance.dispose();
        self.preview.cancel_tooltip();
        self.preferences_flush_requested = true;
        self.preference_debounce.flush();
        self.intent_bridge.enqueue_preferences_ready();
        if let Some(draft) = self.properties_draft.as_mut() {
            draft.action_editor.invalidate_pending_requests();
        }
        self.inspector_action_editor.dispose();
        self.inspector_action_editor_scope = None;
        self.intent_bridge.clear();
        self.stop_native_preview();
        self.preview.dispose();
        self.release_authoring_resources();
        self.open = false;
        self.native_jump_mode.cancel_all();
        self.native_jump_frame = None;
        self.native_combo_anchors.clear();
        self.native_combo_owner = None;
        self.child_lifetimes.clear();
        self.viewport_close_pending = true;
        self.viewport_restore_pending = true;
        self.viewport_focus_pending = false;
        self.session = None;
        self.trace_submission = None;
        self.close_intent = CloseIntent::None;
        self.close_stop_attempted = false;
        self.close_prompt = false;
        self.focus_restore = None;
        self.placement_draft = None;
        self.pending_drop = None;
        self.properties_popup = None;
        self.properties_draft = None;
        self.radial_add_request = None;
        self.radial_add_menu = None;
        self.radial_add_ring = None;
        self.radial_add_cell = None;
        self.radial_add_replace_confirmed = false;
        self.radial_add_close_tree_confirmed = false;
    }

    fn send_commit(&mut self, disposition: CommitDisposition) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let mut delivery_failed = false;
        match session.request_commit(disposition) {
            Ok(request) => {
                let pending = session.pending_request;
                let result = self.client.as_ref().map_or_else(
                    || Err(AuthoringError::ServiceClosed),
                    |client| client.send(request),
                );
                if let Err(error) = result
                    && let Some(pending) = pending
                {
                    session.reconcile_request_delivery_failure(pending, format!("{error:?}"));
                    delivery_failed = true;
                }
            }
            Err(error) => {
                session.last_error = Some(format!("{error:?}"));
                delivery_failed = true;
            }
        }
        if delivery_failed && self.close_intent != CloseIntent::None {
            // Keep the close decision actionable after a failed Save or
            // Discard transaction instead of leaving the editor latched in a
            // non-prompting close state.
            self.close_prompt = true;
        }
    }

    fn retry_authoritative_snapshot(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.pending_request.is_some() {
            return;
        }
        match session.request_snapshot() {
            Ok(request) => {
                let pending = session.pending_request;
                let result = self.client.as_ref().map_or_else(
                    || Err(AuthoringError::ServiceClosed),
                    |client| client.send(request),
                );
                if let Err(error) = result
                    && let Some(pending) = pending
                {
                    session.reconcile_request_delivery_failure(pending, format!("{error:?}"));
                }
            }
            Err(error) => session.last_error = Some(format!("{error:?}")),
        }
    }

    fn properties_popup_dirty(&self) -> bool {
        let inspector_dirty = self.inspector_action_editor.has_unassigned_text();
        let Some(draft) = self.properties_draft.as_ref() else {
            return inspector_dirty;
        };
        let target_differs = if let StableSelection::Cell {
            menu_id,
            ring_id,
            cell_id,
        } = &draft.target
        {
            self.session
                .as_ref()
                .and_then(|session| {
                    session
                        .draft
                        .menus
                        .iter()
                        .find(|menu| &menu.id == menu_id)
                        .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
                        .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == cell_id))
                })
                .is_some_and(|cell| draft.differs_from(cell))
        } else {
            false
        };
        target_differs || draft.action_editor.has_unassigned_text() || inspector_dirty
    }

    fn pending_inspector_action_buffer(&self) -> Option<(BindingEditorScope, ActionBinding)> {
        if !self.inspector_action_editor.has_unassigned_text() {
            return None;
        }
        let scope = self.inspector_action_editor_scope.as_ref()?.clone();
        let session = self.session.as_ref()?;
        if scope.surface != BindingEditorSurface::Inspector
            || scope.editor_session != session.editor_session
            || scope.draft_generation != session.generation
            || session.selection.as_ref() != Some(&scope.target)
        {
            return None;
        }
        let StableSelection::Cell {
            menu_id,
            ring_id,
            cell_id,
        } = &scope.target
        else {
            return None;
        };
        let cell = session
            .draft
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
            .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == cell_id))?;
        let _assigned_binding = match &cell.content {
            CellContent::Action { binding } => Some(binding),
            _ => None,
        };
        Some((
            scope,
            self.inspector_action_editor.binding_for_explicit_apply()?,
        ))
    }

    fn pending_inspector_action_buffer_for_target(
        &self,
        target: &StableSelection,
    ) -> Option<(BindingEditorScope, ActionBinding)> {
        self.pending_inspector_action_buffer()
            .filter(|(scope, _)| &scope.target == target)
    }

    fn apply_inspector_action_buffer(&mut self) -> Result<(), String> {
        let (scope, binding) = self
            .pending_inspector_action_buffer()
            .ok_or_else(|| "Inspector action input is stale or incomplete".to_owned())?;
        let StableSelection::Cell {
            menu_id,
            ring_id,
            cell_id,
        } = &scope.target
        else {
            return Err("Inspector action input no longer targets a cell".into());
        };
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| "Authoring session is no longer available".to_owned())?;
        if session.editor_session != scope.editor_session
            || session.generation != scope.draft_generation
            || session.selection.as_ref() != Some(&scope.target)
        {
            return Err("Inspector action input is stale; keep editing or discard it".into());
        }
        menu::set_cell_content(
            session,
            menu_id,
            ring_id,
            cell_id,
            CellContent::Action {
                binding: binding.clone(),
            },
        )
        .map_err(|error| format!("Action assignment failed: {error:?}"))?;
        self.inspector_action_editor
            .set_assigned_binding(Some(binding));
        Ok(())
    }

    fn discard_inspector_action_buffer(&mut self) {
        let Some(scope) = self.inspector_action_editor_scope.clone() else {
            return;
        };
        let assigned_binding = self.session.as_ref().and_then(|session| {
            if session.editor_session != scope.editor_session {
                return None;
            }
            let StableSelection::Cell {
                menu_id,
                ring_id,
                cell_id,
            } = &scope.target
            else {
                return None;
            };
            session
                .draft
                .menus
                .iter()
                .find(|menu| &menu.id == menu_id)
                .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
                .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == cell_id))
                .and_then(|cell| match &cell.content {
                    CellContent::Action { binding } => Some(binding.clone()),
                    _ => None,
                })
        });
        self.inspector_action_editor
            .discard_unassigned_changes(scope, assigned_binding.as_ref());
    }

    fn reconcile_bulk_edit_stage(&mut self, scope: Option<BulkEditStageScope>) {
        if self.bulk_edit_stage_scope == scope {
            return;
        }
        self.bulk_edit_stage_scope = scope;
        self.bulk_label_value.clear();
        self.bulk_style_stage_key.clear();
        self.bulk_style_mode = BulkStyleMode::Set;
        self.bulk_style_staged_value = None;
        self.bulk_style_text.clear();
        self.bulk_after_action_slot = BulkAfterActionSlot::Primary;
        self.bulk_after_action_policy = AfterActionPolicy::Inherit;
        self.bulk_feedback = None;
    }

    fn inspector_scope_change_guard_ui(
        &mut self,
        ctx: &egui::Context,
        parent_instance: Option<u64>,
        ui_pass: &mut RadialUiPass,
    ) {
        let Some(pending) = self.pending_scope_change.as_ref() else {
            return;
        };
        let current = self.scope_change_is_current(pending);
        let dirty = self.inspector_action_editor.has_unassigned_text();
        if !dirty {
            if let Some(pending) = self.pending_scope_change.take() {
                self.apply_pending_scope_change(pending);
            }
            return;
        }
        let can_apply = current
            && pending.intent.apply_and_continue_is_safe()
            && self.pending_inspector_action_buffer().is_some();
        let apply_only = current
            && !pending.intent.apply_and_continue_is_safe()
            && self.pending_inspector_action_buffer().is_some();
        let title = if current {
            "Unsaved Inspector action"
        } else {
            "Stale Inspector action"
        };
        let window_message = if current {
            "Save or discard the Inspector action input before changing the target or draft."
        } else {
            "The requested change no longer matches the session, generation, or selected target."
        };
        let mut decision = None;
        let subject = radial_value_digest(&(
            pending.anchor.editor_session,
            pending.anchor.generation,
            pending.anchor.selection.as_ref(),
            std::mem::discriminant(&pending.intent),
        ));
        let (window_id, native_owner) = self.radial_child_window_config(
            ui_pass,
            parent_instance,
            RadialDesignerChildKind::ScopeChangeGuard,
            subject,
            egui::Id::new("radial-designer-scope-change-guard"),
        );
        let mut window_open = true;
        let mut visible_commit = false;
        egui::Window::new(title)
            .id(window_id)
            .collapsible(false)
            .resizable(false)
            .open(&mut window_open)
            .show(ctx, |ui| {
                visible_commit = native_owner
                    && ctx.current_pass_index() == 0
                    && ui.is_visible()
                    && !ui.is_sizing_pass();
                ui_pass.commit_allowed = visible_commit;
                ui.add_enabled_ui(native_owner, |ui| {
                    ui.label(window_message);
                    if current && !can_apply && !apply_only {
                        ui.small("The action input is incomplete or its target has changed.");
                    }
                    ui.horizontal_wrapped(|ui| {
                        let apply = ui.add_enabled(
                            can_apply || apply_only,
                            egui::Button::new(if apply_only {
                                "Apply action"
                            } else {
                                "Apply and continue"
                            }),
                        );
                        let apply = with_button_emphasis(ui, apply, ButtonEmphasis::Primary);
                        ui_pass.register(
                            ui,
                            &apply,
                            "scope-change/apply",
                            "Apply action",
                            TargetKind::Button,
                        );
                        if apply.clicked() && visible_commit {
                            decision = Some(PropertiesNavigationDecision::Apply);
                        }
                        let button = ui.button(if current {
                            "Discard and continue"
                        } else {
                            "Discard action and cancel stale change"
                        });
                        let response =
                            with_button_emphasis(ui, button, ButtonEmphasis::Destructive);
                        ui_pass.register(
                            ui,
                            &response,
                            "scope-change/discard",
                            "Discard action",
                            TargetKind::Button,
                        );
                        if response.clicked() && visible_commit {
                            decision = Some(PropertiesNavigationDecision::Discard);
                        }
                        if current {
                            let response = ui.button("Cancel change");
                            ui_pass.register(
                                ui,
                                &response,
                                "scope-change/cancel",
                                "Cancel change",
                                TargetKind::Button,
                            );
                            if response.clicked() && visible_commit {
                                decision = Some(PropertiesNavigationDecision::Cancel);
                            }
                        }
                    });
                });
            });
        if !native_owner || !visible_commit {
            decision = None;
        } else if !window_open {
            decision = Some(PropertiesNavigationDecision::Cancel);
        }
        match decision {
            Some(PropertiesNavigationDecision::Cancel) => {
                self.pending_scope_change = None;
            }
            Some(PropertiesNavigationDecision::Discard) => {
                self.discard_inspector_action_buffer();
                if current {
                    if let Some(pending) = self.pending_scope_change.take() {
                        self.apply_pending_scope_change(pending);
                    }
                } else {
                    self.pending_scope_change = None;
                }
            }
            Some(PropertiesNavigationDecision::Apply) => {
                match self.apply_inspector_action_buffer() {
                    Ok(()) => {
                        if let Some(mut pending) = self.pending_scope_change.take() {
                            if pending.intent.apply_and_continue_is_safe() {
                                self.rebase_scope_change_after_explicit_apply(&mut pending);
                                self.apply_pending_scope_change(pending);
                            } else {
                                self.resource_notice = Some(ResourceNotice::info(
                                    "The action was applied. Repeat the requested change to confirm it.",
                                ));
                            }
                        }
                    }
                    Err(error) => {
                        self.resource_notice = Some(ResourceNotice::warning(error));
                    }
                }
            }
            None => {}
        }
    }

    fn invalidate_binding_editor_scope(&mut self) {
        if let Some(draft) = self.properties_draft.as_mut() {
            draft.action_editor.invalidate_pending_requests();
            draft.action_editor.invalidate_visit();
        }
        self.inspector_action_editor.invalidate_pending_requests();
        self.inspector_action_editor.invalidate_visit();
        self.inspector_action_editor_scope = None;
    }

    fn queue_navigation_intent(&mut self, intent: DesignerNavigationIntent) {
        if let Some(session) = self.session.as_ref() {
            queue_scope_change(
                &mut self.pending_scope_change,
                session,
                ScopeChangeIntent::Navigation(intent),
            );
        }
    }

    fn queue_editor_operation(&mut self, operation: DeferredEditorOperation) {
        if let Some(session) = self.session.as_ref() {
            queue_scope_change(
                &mut self.pending_scope_change,
                session,
                ScopeChangeIntent::Operation(operation),
            );
        }
    }

    fn scope_change_is_current(&self, pending: &PendingScopeChange) -> bool {
        self.session.as_ref().is_some_and(|session| {
            session.editor_session == pending.anchor.editor_session
                && session.generation == pending.anchor.generation
                && session.selection == pending.anchor.selection
        })
    }

    fn rebase_scope_change_after_explicit_apply(&self, pending: &mut PendingScopeChange) {
        let previous_generation = pending.anchor.generation;
        if let Some(session) = self.session.as_ref()
            && session.editor_session == pending.anchor.editor_session
            && session.selection == pending.anchor.selection
        {
            pending.anchor.generation = session.generation;
            if let ScopeChangeIntent::Operation(DeferredEditorOperation::MoveCell {
                pending: cell_drop,
                ..
            }) = &mut pending.intent
                && cell_drop.generation == previous_generation
            {
                cell_drop.generation = session.generation;
            }
        }
    }

    fn apply_pending_scope_change(&mut self, pending: PendingScopeChange) {
        if !self.scope_change_is_current(&pending) {
            self.resource_notice = Some(ResourceNotice::warning(
                "The requested change is stale; review the current selection and try again",
            ));
            return;
        }
        match pending.intent {
            ScopeChangeIntent::Navigation(intent) => self.apply_navigation_intent(intent),
            ScopeChangeIntent::Operation(operation) => {
                self.apply_editor_operation(operation, pending.anchor.generation)
            }
        }
    }

    fn apply_editor_operation(
        &mut self,
        operation: DeferredEditorOperation,
        expected_generation: crate::radial::authoring::DraftGeneration,
    ) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.generation != expected_generation {
            self.resource_notice = Some(ResourceNotice::warning(
                "The requested edit is stale; review the current draft and try again",
            ));
            return;
        }
        let before_selection = session.selection.clone();
        let before_generation = session.generation;
        let mut failure = None;
        let mut radial_insertion_trace = None;
        let mut conflict_resolved = false;
        match operation {
            DeferredEditorOperation::CreateMenu {
                interaction,
                submenu_presentation,
            } => {
                if let Err(error) = menu::create_menu_with_defaults(
                    session,
                    "menu",
                    "New menu",
                    interaction,
                    submenu_presentation,
                ) {
                    failure = Some(format!("New menu failed: {error:?}"));
                }
            }
            DeferredEditorOperation::ApplyRingProposal { proposal, nonce } => {
                let current_proposal = self.ring_proposal.as_ref();
                if self.ring_proposal_nonce != Some(nonce)
                    || current_proposal != Some(proposal.as_ref())
                    || proposal.base_generation != session.generation
                {
                    failure = Some("Ring proposal is stale; prepare it again".into());
                } else {
                    self.ring_proposal = None;
                    self.ring_proposal_nonce = None;
                    if let Err(error) = menu::apply_ring_proposal(session, *proposal) {
                        failure = Some(format!("Proposal apply failed: {error:?}"));
                    }
                }
            }
            DeferredEditorOperation::CopyCell {
                menu_id,
                ring_id,
                cell_id,
            } => {
                if let Err(error) = menu::copy_cell(session, &menu_id, &ring_id, &cell_id) {
                    failure = Some(format!("Copy failed: {error:?}"));
                }
            }
            DeferredEditorOperation::DeleteCell {
                menu_id,
                ring_id,
                cell_id,
            } => {
                if let Err(error) = menu::delete_cell(session, &menu_id, &ring_id, &cell_id) {
                    failure = Some(format!("Delete failed: {error:?}"));
                }
            }
            DeferredEditorOperation::CreateSubmenuAndLink {
                menu_id,
                ring_id,
                cell_id,
                name,
            } => {
                if let Err(error) =
                    menu::create_submenu_and_link(session, &menu_id, &ring_id, &cell_id, &name)
                {
                    failure = Some(format!("Create submenu failed: {error:?}"));
                }
            }
            DeferredEditorOperation::LinkExistingSubmenu {
                menu_id,
                ring_id,
                cell_id,
                linked_menu,
            } => {
                if let Err(error) =
                    menu::link_existing_submenu(session, &menu_id, &ring_id, &cell_id, &linked_menu)
                {
                    failure = Some(format!("Link submenu failed: {error:?}"));
                }
            }
            DeferredEditorOperation::CloneSubmenuAndLink {
                menu_id,
                ring_id,
                cell_id,
                linked_menu,
            } => match menu::duplicate_menu(
                session,
                &linked_menu,
                SubmenuDuplication::CloneSubmenuClosure,
            ) {
                Ok(cloned) => {
                    if let Err(error) = menu::set_cell_content(
                        session,
                        &menu_id,
                        &ring_id,
                        &cell_id,
                        CellContent::Submenu { menu_id: cloned },
                    ) {
                        failure = Some(format!("Clone submenu link failed: {error:?}"));
                    }
                }
                Err(error) => failure = Some(format!("Clone submenu failed: {error:?}")),
            },
            DeferredEditorOperation::MoveCell {
                pending,
                resolution,
            } => {
                if pending.generation != session.generation {
                    failure = Some("Cell move is stale; release the drag and try again".into());
                } else {
                    match menu::move_cell_to_slot(
                        session,
                        (
                            &pending.source_menu,
                            &pending.source_ring,
                            &pending.source_cell,
                        ),
                        (
                            &pending.destination_menu,
                            &pending.destination_ring,
                            pending.destination_index,
                        ),
                        resolution,
                        pending.generation,
                    ) {
                        Ok(()) => {
                            self.projected_selection = None;
                            if resolution == menu::CellDropResolution::Swap {
                                self.pending_drop = None;
                            }
                        }
                        Err(menu::MenuEditError::DestinationOccupied)
                            if resolution == menu::CellDropResolution::MoveIntoSpacer =>
                        {
                            self.pending_drop = Some(pending);
                        }
                        Err(error) => {
                            failure = Some(format!("Cell move failed: {error:?}"));
                        }
                    }
                }
            }
            DeferredEditorOperation::SetCellContent {
                menu_id,
                ring_id,
                cell_id,
                content,
            } => {
                if let Err(error) =
                    menu::set_cell_content(session, &menu_id, &ring_id, &cell_id, content)
                {
                    failure = Some(format!("Cell content change failed: {error:?}"));
                } else {
                    self.placement_draft = None;
                }
            }
            DeferredEditorOperation::Undo => {
                if session.undo() {
                    radial_insertion_trace = Some(RadialInsertionControl::Undo);
                } else {
                    failure = Some("Nothing to undo".into());
                }
            }
            DeferredEditorOperation::Redo => {
                if session.redo() {
                    radial_insertion_trace = Some(RadialInsertionControl::Redo);
                } else {
                    failure = Some("Nothing to redo".into());
                }
            }
            DeferredEditorOperation::ResolveConflict(resolution) => {
                if let Err(error) = session.resolve_conflict(resolution) {
                    failure = Some(format!("Conflict resolution failed: {error:?}"));
                } else {
                    conflict_resolved = true;
                }
            }
        }
        if let Some(message) = failure {
            self.resource_notice = Some(ResourceNotice::warning(message));
            return;
        }
        let after_generation = session.generation;
        let after_selection = session.selection.clone();
        let after_document = session.draft.clone();
        if let Some(control) = radial_insertion_trace {
            self.trace_last_radial_add_operation(control);
        }
        if conflict_resolved {
            self.preview.cancel_tooltip();
        }
        if after_generation != before_generation || after_selection != before_selection {
            self.invalidate_binding_editor_scope();
            self.properties_popup = None;
            self.properties_draft = None;
            self.properties_handoff_prompt = false;
            self.placement_draft = None;
            self.reconcile_bulk_edit_stage(None);
            self.projected_selection = None;
            self.focus_restore = after_selection;
            self.visited_path.validate_document_path(&after_document);
        }
    }

    fn apply_navigation_intent(&mut self, intent: DesignerNavigationIntent) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let before = session.selection.clone();
        let document = session.draft.clone();
        let generation = session.generation;
        let mut open_properties = false;
        let mut focus_target = None;
        let mut next_selection = None;
        match intent {
            DesignerNavigationIntent::Select {
                target,
                control,
                shift,
                force_history,
                open_properties: should_open_properties,
            } => {
                let target_menu = match &target {
                    StableSelection::Menu(menu_id)
                    | StableSelection::Ring { menu_id, .. }
                    | StableSelection::Cell { menu_id, .. } => Some(menu_id.clone()),
                    StableSelection::CellSet(cells) => Some(cells.primary.menu_id.clone()),
                    _ => None,
                };
                if let Some(menu_id) = target_menu {
                    let previous_menu = self.visited_path.current().cloned();
                    if force_history || previous_menu.as_ref() != Some(&menu_id) {
                        self.visited_path
                            .direct_reveal(menu_id, before.clone(), force_history);
                    }
                }
                if let StableSelection::Cell {
                    menu_id,
                    ring_id,
                    cell_id,
                } = &target
                {
                    session.select_authored_cell(
                        crate::radial::authoring::AuthoredCellTarget {
                            menu_id: menu_id.clone(),
                            ring_id: ring_id.clone(),
                            cell_id: cell_id.clone(),
                        },
                        control,
                        shift,
                    );
                } else {
                    session.select(Some(target.clone()));
                }
                self.projected_selection = None;
                open_properties =
                    should_open_properties && matches!(target, StableSelection::Cell { .. });
                focus_target = Some(target);
                next_selection = session.selection.clone();
            }
            DesignerNavigationIntent::ClearSelection => {
                session.select(None);
                next_selection = session.selection.clone();
            }
            DesignerNavigationIntent::CreateSkin => {
                match skin_editor::create_skin(session, "New skin") {
                    Ok(skin_id) => {
                        let target = StableSelection::Skin(skin_id);
                        focus_target = Some(target.clone());
                        next_selection = Some(target);
                    }
                    Err(error) => {
                        self.resource_notice = Some(ResourceNotice::error(error));
                        next_selection = session.selection.clone();
                    }
                }
            }
            DesignerNavigationIntent::DuplicateSkin(skin_id) => {
                match skin_editor::duplicate_skin(session, &skin_id) {
                    Ok(duplicate_id) => {
                        let target = StableSelection::Skin(duplicate_id);
                        focus_target = Some(target.clone());
                        next_selection = Some(target);
                    }
                    Err(error) => {
                        self.resource_notice = Some(ResourceNotice::error(error));
                        next_selection = session.selection.clone();
                    }
                }
            }
            DesignerNavigationIntent::EnterSubmenu {
                edge,
                return_selection,
            } => {
                if self
                    .visited_path
                    .enter_submenu(edge.clone(), Some(return_selection))
                {
                    let target = StableSelection::Menu(edge.child_menu);
                    session.select(Some(target.clone()));
                    focus_target = Some(target);
                    next_selection = session.selection.clone();
                }
            }
            DesignerNavigationIntent::Back => {
                if let Some((menu_id, selection)) = self.visited_path.back_location(before.clone())
                {
                    session.select(Some(selection.unwrap_or(StableSelection::Menu(menu_id))));
                    next_selection = session.selection.clone();
                    focus_target = next_selection.clone();
                }
            }
            DesignerNavigationIntent::Breadcrumb(index) => {
                if let Some((menu_id, selection)) = self.visited_path.go_to_breadcrumb(index) {
                    session.select(Some(selection.unwrap_or(StableSelection::Menu(menu_id))));
                    next_selection = session.selection.clone();
                    focus_target = next_selection.clone();
                }
            }
        }
        self.visited_path.validate_document_path(&document);
        let scope_changed = before != next_selection;
        if scope_changed {
            self.invalidate_binding_editor_scope();
            self.properties_popup = None;
            self.properties_draft = None;
            self.properties_handoff_prompt = false;
            self.ring_proposal = None;
            self.ring_proposal_nonce = None;
            self.reconcile_bulk_edit_stage(None);
        }
        if let Some(target) = focus_target {
            if let Some(menu_id) = match &target {
                StableSelection::Menu(menu_id)
                | StableSelection::Ring { menu_id, .. }
                | StableSelection::Cell { menu_id, .. } => Some(menu_id),
                StableSelection::CellSet(cells) => Some(&cells.primary.menu_id),
                _ => None,
            } {
                let expansion_key = format!("menu:{menu_id}");
                let was_open = self
                    .preferences
                    .expanded_sections
                    .insert(expansion_key, true);
                if was_open != Some(true) {
                    self.mark_preferences_changed();
                }
                if was_open == Some(false) {
                    // egui retains an explicit collapsed value under the old
                    // widget ID. Re-keying the tree lets the recorded open
                    // preference become the new widget's default state.
                    self.tree_widget_epoch = self.tree_widget_epoch.wrapping_add(1);
                }
            }
            self.focus_restore = Some(target.clone());
            if open_properties
                && let StableSelection::Cell {
                    menu_id,
                    ring_id,
                    cell_id,
                } = target
            {
                let selection = StableSelection::Cell {
                    menu_id: menu_id.clone(),
                    ring_id: ring_id.clone(),
                    cell_id: cell_id.clone(),
                };
                self.properties_popup = Some(selection);
                if document
                    .menus
                    .iter()
                    .find(|menu| menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
                    .and_then(|ring| ring.cells.iter().find(|cell| cell.id == cell_id))
                    .is_some_and(|cell| matches!(&cell.content, CellContent::Spacer))
                {
                    self.placement_draft =
                        Some(PlacementDraft::new(menu_id, ring_id, cell_id, generation));
                } else {
                    self.placement_draft = None;
                }
            }
        }
    }

    fn poll_replies(&mut self) {
        let session_closed = {
            let Some(session) = self.session.as_mut() else {
                return;
            };
            if let Some(client) = &self.client {
                while let Some(reply) = client.try_recv_gallery() {
                    self.appearance.accept(reply, session);
                }
                while let Some(reply) = client.try_recv() {
                    session.accept_reply(reply);
                }
            }
            if self.close_intent == CloseIntent::None
                && !session.font_catalog_loaded
                && !session.is_initial_snapshot_pending()
                && session.pending_request.is_none()
            {
                match session.request_font_catalog() {
                    Ok(request) => {
                        let pending = session.pending_request;
                        let result = self.client.as_ref().map_or_else(
                            || Err(AuthoringError::ServiceClosed),
                            |client| client.send(request),
                        );
                        if let Err(error) = result
                            && let Some(pending) = pending
                        {
                            session
                                .reconcile_request_delivery_failure(pending, format!("{error:?}"));
                        }
                    }
                    Err(error) => session.last_error = Some(format!("{error:?}")),
                }
            }
            if let (Some(bytes), Some(path)) = (
                session.exported_package.take(),
                self.export_destination.take(),
            ) {
                match crate::common::atomic_file::save_atomic(&path, &bytes) {
                    Ok(()) => {
                        self.resource_notice = Some(ResourceNotice::info(format!(
                            "Exported package to {}",
                            path.display()
                        )))
                    }
                    Err(error) => {
                        self.resource_notice = Some(ResourceNotice::error(error.to_string()))
                    }
                }
            }
            session.is_closed()
        };
        if session_closed {
            self.pending_hotkey_directory_navigation = None;
            self.finish_close();
        } else {
            self.maybe_finish_close();
            self.apply_pending_hotkey_directory_navigation();
        }
    }

    fn apply_pending_hotkey_directory_navigation(&mut self) {
        let Some(pending) = self.pending_hotkey_directory_navigation.as_mut() else {
            return;
        };
        if !self.open || self.close_intent != CloseIntent::None {
            self.pending_hotkey_directory_navigation = None;
            return;
        }
        let Some(session) = self.session.as_ref() else {
            pending.frames_remaining = pending.frames_remaining.saturating_sub(1);
            return;
        };
        if session.is_initial_snapshot_pending() {
            if pending.frames_remaining == 0 {
                self.pending_hotkey_directory_navigation = None;
                self.resource_notice = Some(ResourceNotice::warning(
                    "The radial destination did not finish loading; no selection was changed.",
                ));
            } else {
                pending.frames_remaining -= 1;
            }
            return;
        }

        let expected_revision = match &pending.target {
            HotkeyDirectoryRadialTarget::Trigger {
                source_revision, ..
            }
            | HotkeyDirectoryRadialTarget::Shortcut {
                source_revision, ..
            } => *source_revision,
        };
        if session.baseline.revision.0 != expected_revision {
            self.pending_hotkey_directory_navigation = None;
            self.resource_notice = Some(ResourceNotice::warning(
                "The radial configuration changed before its destination opened.",
            ));
            return;
        }
        let selection = match &pending.target {
            HotkeyDirectoryRadialTarget::Trigger {
                trigger_id,
                menu_id,
                ..
            } => session
                .draft
                .custom_triggers
                .iter()
                .any(|trigger| {
                    trigger.id.as_str() == trigger_id && trigger.menu_id.as_str() == menu_id
                })
                .then(|| StableSelection::Menu(MenuId::new(menu_id.clone()))),
            HotkeyDirectoryRadialTarget::Shortcut {
                menu_id,
                cell_id,
                shortcut_id,
                ..
            } => session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id.as_str() == menu_id)
                .and_then(|menu| {
                    menu.rings.iter().find_map(|ring| {
                        ring.cells
                            .iter()
                            .find(|cell| cell.id.as_str() == cell_id)
                            .filter(|cell| {
                                cell.shortcuts
                                    .iter()
                                    .any(|shortcut| shortcut.id.as_str() == shortcut_id)
                            })
                            .map(|cell| StableSelection::Cell {
                                menu_id: menu.id.clone(),
                                ring_id: ring.id.clone(),
                                cell_id: cell.id.clone(),
                            })
                    })
                }),
        };
        let Some(selection) = selection else {
            self.pending_hotkey_directory_navigation = None;
            self.resource_notice = Some(ResourceNotice::warning(
                "The radial destination is no longer present; no selection was changed.",
            ));
            return;
        };
        self.pending_hotkey_directory_navigation = None;
        self.queue_navigation_intent(DesignerNavigationIntent::Select {
            target: selection,
            control: false,
            shift: false,
            force_history: false,
            open_properties: false,
        });
    }

    fn release_authoring_resources(&mut self) {
        if self.preferences_dirty {
            // Preserve the final close flush before unregistering the wake
            // callbacks.  ROOT can consume the bit after this viewport has
            // disposed its authoring resources.
            self.intent_bridge.enqueue_preferences_ready();
        }
        self.intent_bridge.set_wake(None);
        self.intent_bridge.set_viewport_wake(None);
        self.intent_bridge.clear();
        if let (Some(client), Some(session)) = (&self.client, &self.session) {
            client.set_reply_wake(None);
            client.release_resources(session.editor_session());
        }
        self.client = None;
    }

    fn send_native_preview(&mut self, update: bool) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let menu_id =
            selected_menu_id(session).unwrap_or_else(|| session.draft.default_menu_id.clone());
        let selected_skin = match session.selection.as_ref() {
            Some(StableSelection::Skin(id)) => Some(id.clone()),
            _ => None,
        };
        let request = if update {
            session.request_update_native_preview(
                menu_id,
                self.sample_native_context,
                selected_skin,
            )
        } else {
            session.request_start_native_preview(menu_id, self.sample_native_context, selected_skin)
        };
        match request {
            Ok(request) => {
                let pending = session.pending_native_preview;
                let result = self.client.as_ref().map_or_else(
                    || Err(AuthoringError::ServiceClosed),
                    |client| client.send(request),
                );
                if let Err(error) = result
                    && let Some(pending) = pending
                {
                    session.reconcile_request_delivery_failure(pending, format!("{error:?}"));
                }
            }
            Err(error) => session.last_error = Some(format!("{error:?}")),
        }
    }

    fn stop_native_preview(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let request = session.request_stop_native_preview();
        match request {
            Ok(request) => {
                let pending = session.pending_native_preview;
                let result = self.client.as_ref().map_or_else(
                    || Err(AuthoringError::ServiceClosed),
                    |client| client.send(request),
                );
                if self.close_intent != CloseIntent::None {
                    self.close_stop_attempted = true;
                }
                if let Err(error) = result
                    && let Some(pending) = pending
                {
                    session.reconcile_request_delivery_failure(pending, format!("{error:?}"));
                }
            }
            Err(error) => {
                if self.close_intent != CloseIntent::None {
                    self.close_stop_attempted = true;
                }
                session.last_error = Some(format!("{error:?}"));
            }
        }
    }

    fn sync_native_preview_generation(&mut self) {
        let needs_stop = self.session.as_ref().is_some_and(|session| {
            session.pending_native_preview.is_none()
                && session.native_preview_may_be_open
                && session.native_preview_lease.is_none()
        });
        if needs_stop {
            self.stop_native_preview();
            return;
        }
        let needs_update = self.session.as_ref().is_some_and(|session| {
            session.pending_native_preview.is_none()
                && session
                    .native_preview_lease
                    .as_ref()
                    .is_some_and(|lease| lease.generation != session.generation)
        });
        if needs_update {
            self.send_native_preview(true);
        }
    }

    fn current_native_combo_popup_layer(
        &self,
        ctx: &egui::Context,
        binding: NavigationViewportBinding,
    ) -> Option<egui::LayerId> {
        if !egui::Popup::is_any_open(ctx) {
            return None;
        }

        self.native_combo_anchors.iter().find_map(|anchor| {
            if anchor.viewport != binding.viewport || anchor.layer_id != binding.base_layer {
                return None;
            }
            let response = ctx.read_response(anchor.response_id)?;
            if response.ctx.viewport_id() != binding.viewport
                || !response.enabled()
                || response.layer_id != anchor.layer_id
                || !response.interact_rect.is_positive()
                || ctx.layer_id_at(response.interact_rect.center()) != Some(anchor.layer_id)
                || !egui::ComboBox::is_open(ctx, response.id)
            {
                return None;
            }

            let popup_id = egui::Popup::default_response_id(&response);
            if !egui::Popup::is_id_open(ctx, popup_id) {
                return None;
            }
            let popup_rect = ctx.memory(|memory| memory.area_rect(popup_id))?;
            if !popup_rect.is_positive() {
                return None;
            }

            let popup_layer = egui::LayerId::new(egui::Order::Foreground, popup_id);
            let popup_is_current = ctx.layer_id_at(popup_rect.center()) == Some(popup_layer);
            let popup_is_sizing = egui::containers::AreaState::load(ctx, popup_id)
                .is_some_and(|state| !state.interactable);
            (popup_is_current || popup_is_sizing).then_some(popup_layer)
        })
    }

    fn active_radial_child_keys(&self) -> HashSet<RadialChildLifetimeKey> {
        let mut keys = HashSet::new();
        if let Some(selection) = self.properties_popup.as_ref() {
            keys.insert(RadialChildLifetimeKey {
                kind: RadialDesignerChildKind::CellProperties,
                subject: radial_selection_digest(selection),
            });
        }
        if let Some(pending) = self.pending_scope_change.as_ref() {
            keys.insert(RadialChildLifetimeKey {
                kind: RadialDesignerChildKind::ScopeChangeGuard,
                subject: radial_value_digest(&(
                    pending.anchor.editor_session,
                    pending.anchor.generation,
                    pending.anchor.selection.as_ref(),
                    std::mem::discriminant(&pending.intent),
                )),
            });
        }
        if let Some(request) = self.radial_add_request.as_ref() {
            keys.insert(RadialChildLifetimeKey {
                kind: RadialDesignerChildKind::AddToRadial,
                subject: request.trace_identity.request_id,
            });
        }
        if self.close_prompt {
            keys.insert(RadialChildLifetimeKey {
                kind: RadialDesignerChildKind::ClosePrompt,
                subject: 0,
            });
        }
        if self.delete_message.is_some() {
            keys.insert(RadialChildLifetimeKey {
                kind: RadialDesignerChildKind::DeleteMessage,
                subject: 0,
            });
        }
        if let Some((menu_id, ring_id)) = self.delete_ring_prompt.as_ref() {
            keys.insert(RadialChildLifetimeKey {
                kind: RadialDesignerChildKind::DeleteRing,
                subject: radial_value_digest(&(menu_id, ring_id)),
            });
        }
        if let Some(plan) = self.resize_prompt.as_ref() {
            keys.insert(RadialChildLifetimeKey {
                kind: RadialDesignerChildKind::ResizeResolution,
                subject: radial_value_digest(&(&plan.menu_id, &plan.ring_id, plan.requested_len)),
            });
        }
        keys
    }

    fn reconcile_radial_child_lifetimes(&mut self) {
        let active = self.active_radial_child_keys();
        self.child_lifetimes.retain(|key, _| active.contains(key));
    }

    fn radial_child_owner(
        &mut self,
        parent_instance: u64,
        key: RadialChildLifetimeKey,
    ) -> NavigationOwner {
        let child_instance = if let Some(instance) = self.child_lifetimes.get(&key) {
            *instance
        } else {
            let instance = self.next_child_lifetime.max(1);
            self.next_child_lifetime = instance.wrapping_add(1).max(1);
            self.child_lifetimes.insert(key, instance);
            instance
        };
        NavigationOwner::RadialDesignerChild {
            parent_instance,
            child_instance,
            kind: key.kind,
            subject: key.subject,
        }
    }

    fn active_radial_child_owners(&mut self, parent_instance: u64) -> Vec<NavigationOwner> {
        let mut keys = self
            .active_radial_child_keys()
            .into_iter()
            .collect::<Vec<_>>();
        keys.sort_by_key(|key| (key.kind as u8, key.subject));
        keys.into_iter()
            .map(|key| self.radial_child_owner(parent_instance, key))
            .collect()
    }

    fn radial_child_binding(
        owner: NavigationOwner,
        viewport: egui::ViewportId,
    ) -> Option<NavigationViewportBinding> {
        let NavigationOwner::RadialDesignerChild {
            parent_instance,
            child_instance,
            kind,
            subject,
        } = owner
        else {
            return None;
        };
        Some(NavigationViewportBinding {
            viewport,
            owner,
            base_layer: radial_designer_child_layer_id(
                parent_instance,
                child_instance,
                kind,
                subject,
            ),
        })
    }

    fn radial_owner_layer(owner: NavigationOwner) -> Option<egui::LayerId> {
        let NavigationOwner::RadialDesignerChild {
            parent_instance,
            child_instance,
            kind,
            subject,
        } = owner
        else {
            return None;
        };
        Some(radial_designer_child_layer_id(
            parent_instance,
            child_instance,
            kind,
            subject,
        ))
    }

    fn radial_child_window_config(
        &mut self,
        ui_pass: &RadialUiPass,
        parent_instance: Option<u64>,
        kind: RadialDesignerChildKind,
        subject: u64,
        legacy_id: egui::Id,
    ) -> (egui::Id, bool) {
        let Some(parent_instance) = parent_instance else {
            return (legacy_id, ui_pass.native_ui_allowed);
        };
        let owner =
            self.radial_child_owner(parent_instance, RadialChildLifetimeKey { kind, subject });
        let NavigationOwner::RadialDesignerChild { child_instance, .. } = owner else {
            return (legacy_id, false);
        };
        (
            radial_designer_child_window_id(parent_instance, child_instance, kind, subject),
            ui_pass.owns_child(parent_instance, child_instance, kind, subject)
                && ui_pass.native_ui_allowed,
        )
    }

    fn viewport_ui(
        &mut self,
        ui: &mut egui::Ui,
        frame: &DesignerFrameContext,
        viewport_class: egui::ViewportClass,
    ) {
        if viewport_class == egui::ViewportClass::EmbeddedWindow {
            self.native_jump_mode.cancel_all();
            self.native_jump_frame = None;
            return;
        }
        let ctx = ui.ctx().clone();
        let viewport = radial_designer_viewport_id();
        let parent_binding = frame
            .navigation_instance
            .map(|instance| NavigationViewportBinding {
                viewport,
                owner: NavigationOwner::Dialog {
                    kind: NavigationDialogKind::RadialDesigner,
                    instance,
                },
                base_layer: egui::LayerId::background(),
            });
        self.reconcile_radial_child_lifetimes();
        let child_owners = parent_binding.map_or_else(Vec::new, |binding| {
            let NavigationOwner::Dialog { instance, .. } = binding.owner else {
                return Vec::new();
            };
            self.active_radial_child_owners(instance)
        });
        let top_layer = visible_top_middle_layer(&ctx);
        let selected_child = child_owners
            .iter()
            .copied()
            .find(|owner| Self::radial_owner_layer(*owner) == top_layer)
            .or_else(|| child_owners.first().copied());
        let binding = selected_child
            .and_then(|owner| Self::radial_child_binding(owner, viewport))
            .or(parent_binding);
        let combo_owner = binding.and_then(|binding| {
            self.session
                .as_ref()
                .map(|session| (binding.owner, session.editor_session.0))
        });
        if self.native_combo_owner != combo_owner {
            self.native_combo_anchors.clear();
            self.native_combo_owner = combo_owner;
        }
        self.native_jump_mode.set_enabled(frame.jump_mode_enabled);
        let frame_nr = ctx.cumulative_frame_nr();
        if self.native_jump_frame != Some(frame_nr) {
            self.native_jump_mode.begin_frame();
            self.native_jump_frame = Some(frame_nr);
        }
        let owned_popup_layer =
            binding.and_then(|binding| self.current_native_combo_popup_layer(&ctx, binding));
        let native_popup_is_owned = !egui::Popup::is_any_open(&ctx) || owned_popup_layer.is_some();
        let native_focus_is_owned =
            binding.is_some_and(|binding| binding.focus_is_owned(&ctx, owned_popup_layer));
        let native_ui_owner_current = frame.application_input_allowed
            && !frame.intent_bridge.has_pending_file_dialog()
            && binding.is_some_and(|binding| binding.base_is_current(&ctx))
            && native_popup_is_owned
            && native_focus_is_owned;
        let navigation_owner = binding.map_or(NavigationOwner::Blocked, |binding| {
            self.native_jump_mode.update_navigation_owner_for_viewport(
                &ctx,
                binding,
                frame.application_input_allowed && frame.jump_mode_enabled,
            )
        });
        let jump_owner_current = binding.is_some_and(|binding| {
            navigation_owner == binding.owner
                && self.native_jump_mode.frame_navigation_owner() == binding.owner
                && native_ui_owner_current
        });
        let first_ui_pass = ctx.current_pass_index() == 0;
        self.native_jump_mode.handle_viewport_input(
            &ctx,
            jump_owner_current,
            first_ui_pass,
            viewport,
        );
        self.native_jump_mode.begin_ui_pass();
        let target_scope = binding
            .filter(|_| jump_owner_current && frame.jump_mode_enabled)
            .and_then(|binding| {
                let owner = binding.owner;
                let session = self.session.as_ref()?;
                match owner {
                    NavigationOwner::Dialog { instance, .. } => {
                        let target_generation = radial_jump_target_generation(
                            instance,
                            session.editor_session,
                            session.generation,
                        );
                        self.native_jump_mode.scope_for_viewport_binding(
                            SurfaceId::dialog(
                                NavigationDialogKind::RadialDesigner.surface_kind(),
                                instance,
                            ),
                            target_generation,
                            "radial-designer",
                            binding,
                        )
                    }
                    NavigationOwner::RadialDesignerChild {
                        parent_instance,
                        child_instance,
                        kind,
                        subject,
                    } => {
                        let target_generation = radial_jump_target_generation(
                            child_instance,
                            session.editor_session,
                            session.generation,
                        );
                        self.native_jump_mode.scope_for_viewport_binding(
                            SurfaceId::popup(
                                kind.surface_kind(),
                                child_instance,
                                SurfaceId::dialog(
                                    NavigationDialogKind::RadialDesigner.surface_kind(),
                                    parent_instance,
                                ),
                            ),
                            TargetGeneration(target_generation.0 ^ subject),
                            format!("radial-designer-child/{:?}", kind),
                            binding,
                        )
                    }
                    _ => None,
                }
            });
        let mut radial_ui_pass = RadialUiPass::new(
            target_scope,
            binding.map_or(NavigationOwner::Blocked, |binding| binding.owner),
            native_ui_owner_current,
            native_ui_owner_current && first_ui_pass,
        );
        // Eframe initializes native AccessKit on the root viewport only. The
        // deferred Designer still needs to publish its semantic widget tree.
        ctx.enable_accesskit();
        if acceptance_trace::enabled() {
            let pointer_move = ctx.input(|input| {
                input.events.iter().rev().find_map(|event| match event {
                    egui::Event::PointerMoved(position) => Some(*position),
                    _ => None,
                })
            });
            if let Some(position) = pointer_move {
                let scale = ctx.pixels_per_point();
                let to_pixel = |coordinate: f32| {
                    let pixel = coordinate * scale;
                    (pixel.is_finite() && pixel >= i32::MIN as f32 && pixel <= i32::MAX as f32)
                        .then(|| pixel.round() as i32)
                };
                if let Some((client_x, client_y)) = to_pixel(position.x).zip(to_pixel(position.y)) {
                    acceptance_trace::emit(Event::DesignerPointerMoved { client_x, client_y });
                }
            }
        }
        let (pointer_down, pointer_up) =
            ctx.input(|input| (input.pointer.any_pressed(), input.pointer.any_released()));
        if pointer_down || pointer_up {
            acceptance_trace::emit(Event::DesignerPointer {
                down: pointer_down,
                up: pointer_up,
                window_under_cursor: crate::window_manager::window_under_cursor(),
                correlation: trace_correlation(self.session.as_ref()),
            });
        }
        self.poll_replies();
        if !self.show_resources {
            self.appearance.retire_interest();
        }
        if self.close_intent == CloseIntent::None {
            self.sync_native_preview_generation();
        }
        if !self.open {
            self.native_jump_mode.cancel_all();
            self.native_jump_frame = None;
            self.native_combo_anchors.clear();
            self.native_combo_owner = None;
            self.child_lifetimes.clear();
            return;
        }
        if let Some(document) = self.session.as_ref().map(|session| session.draft.clone()) {
            self.visited_path.validate_document_path(&document);
        }
        let selection_before = self
            .session
            .as_ref()
            .and_then(|session| session.selection.clone());
        let tooltip_preferences =
            crate::radial::tooltip::TooltipPreferences::from(&frame.feature_defaults);
        let (
            conflict_reason,
            preview_selection,
            prepared_preview,
            prepared_menu,
            prepared_appearance,
            draft,
            generation,
            editor_session,
            initial_snapshot_pending,
            show_expected_layout_diagnostics,
        ) = match self.session.as_mut() {
            Some(session) => {
                let conflict_reason = session
                    .conflict
                    .as_ref()
                    .map(|conflict| conflict.reason.clone());
                if self.designer_mode == DesignerMode::Design {
                    let selected_menu =
                        session
                            .selection
                            .as_ref()
                            .and_then(|selection| match selection {
                                StableSelection::Menu(menu_id) => Some(menu_id.clone()),
                                StableSelection::Ring { menu_id, .. }
                                | StableSelection::Cell { menu_id, .. } => Some(menu_id.clone()),
                                StableSelection::CellSet(cells) => {
                                    Some(cells.primary.menu_id.clone())
                                }
                                _ => None,
                            });
                    if let Some(selected_menu) = selected_menu.filter(|menu_id| {
                        session.draft.menus.iter().any(|menu| &menu.id == menu_id)
                    }) && self.visited_path.current() != Some(&selected_menu)
                    {
                        self.visited_path.direct_reveal(selected_menu, None, false);
                    }
                }
                let appearance_candidate = self.appearance.candidate(session);
                let proposal = self
                    .ring_proposal
                    .as_ref()
                    .filter(|proposal| proposal.base_generation == session.generation);
                let preview_selection = appearance_candidate
                    .as_ref()
                    .map(|(_, _, selection)| Some(selection.clone()))
                    .unwrap_or_else(|| {
                        proposal.map_or_else(
                            || session.selection.clone(),
                            |proposal| {
                                Some(StableSelection::Ring {
                                    menu_id: proposal.menu_id.clone(),
                                    ring_id: proposal.ring_id.clone(),
                                })
                            },
                        )
                    });
                let preparation_preset = if self.designer_mode == DesignerMode::Design {
                    PreviewPreset::Current
                } else {
                    self.preview_preset
                };
                let proposal_token = proposal
                    .zip(self.ring_proposal_nonce)
                    .map(|(proposal, nonce)| ring_proposal_token(proposal, nonce));
                self.preview.sync_preparation(
                    session,
                    self.client.as_ref(),
                    preparation_preset,
                    preview_selection.as_ref(),
                    tooltip_preferences,
                    appearance_candidate
                        .as_ref()
                        .map(|(candidate, token, _)| (&**candidate, token.as_str()))
                        .or_else(|| {
                            proposal
                                .zip(proposal_token.as_deref())
                                .map(|(proposal, token)| (&proposal.document, token))
                        }),
                );
                let prepared_preview = self.preview.prepared_frame(session);
                let prepared_menu = self.preview.current_menu().cloned();
                let prepared_appearance = self.appearance.prepared_identity(session, &self.preview);
                let draft = appearance_candidate
                    .as_ref()
                    .map(|(candidate, _, _)| Arc::clone(candidate))
                    .unwrap_or_else(|| {
                        proposal.map_or_else(
                            || session.draft.clone(),
                            |proposal| std::sync::Arc::new(proposal.document.clone()),
                        )
                    });
                let generation = session.generation.0;
                let editor_session = session.editor_session;
                let initial_snapshot_pending = session.is_initial_snapshot_pending();
                let show_expected_layout_diagnostics =
                    frame.feature_defaults.show_expected_layout_diagnostics;
                (
                    conflict_reason,
                    preview_selection,
                    prepared_preview,
                    prepared_menu,
                    prepared_appearance,
                    draft,
                    generation,
                    editor_session,
                    initial_snapshot_pending,
                    show_expected_layout_diagnostics,
                )
            }
            None => {
                egui::CentralPanel::default().show(ui, |ui| ui.spinner());
                return;
            }
        };

        if !initial_snapshot_pending
            && self.last_radial_add_reopen_pending
            && let Some(trace) = self.last_radial_add_trace.as_ref()
            && let Some(session) = self.session.as_ref()
        {
            let document_digest_after = radial_add_document_digest(session);
            let binding_digest_after = radial_add_cell_binding_digest(
                session,
                Some(&trace.menu_id),
                Some(&trace.ring_id),
                Some(&trace.cell_id),
            );
            emit_radial_add_control(
                trace.identity,
                RadialInsertionControl::Reopen,
                Some(session),
                Some(&trace.menu_id),
                Some(&trace.ring_id),
                Some(&trace.cell_id),
                true,
                true,
                true,
                document_digest_after,
                binding_digest_after,
            );
            self.last_radial_add_reopen_pending = false;
        }

        let body_state = if initial_snapshot_pending {
            BodyBlock::InitialSnapshot
        } else if conflict_reason.is_some() {
            BodyBlock::Conflict
        } else {
            BodyBlock::Enabled
        };
        if pointer_down || pointer_up {
            acceptance_trace::emit(Event::DesignerBody {
                state: body_state,
                correlation: trace_correlation(self.session.as_ref()),
            });
        }

        let expanded_sections_before = self.preferences.expanded_sections.clone();
        let mut designer_body = |ui: &mut egui::Ui, ui_pass: &mut RadialUiPass| {
            self.toolbar(ui, ui_pass);
            ui.separator();
            if let Some(reason) = conflict_reason.as_deref() {
                self.conflict_ui(ui, reason);
                ui.separator();
            }
            self.preview_controls(ui, show_expected_layout_diagnostics, ui_pass);
            ui.separator();
            if initial_snapshot_pending {
                ui.horizontal_wrapped(|ui| {
                    ui.spinner();
                    ui.label("Loading the authoritative radial configuration…");
                    let can_retry = self
                        .session
                        .as_ref()
                        .is_some_and(|session| session.pending_request.is_none());
                    if ui
                        .add_enabled(can_retry, egui::Button::new("Retry snapshot"))
                        .clicked()
                    {
                        self.retry_authoritative_snapshot();
                    }
                    if ui.button("Close Designer").clicked() {
                        self.request_close();
                    }
                });
                if let Some(error) = self
                    .session
                    .as_ref()
                    .and_then(|session| session.last_error.as_deref())
                {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            }
            show_radial_diagnostics(
                ui,
                frame.expected_diagnostics.iter(),
                show_expected_layout_diagnostics,
                "runtime",
            );
            ui.add_enabled_ui(
                !initial_snapshot_pending
                    && conflict_reason.is_none()
                    && self.pending_scope_change.is_none(),
                |ui| {
                    self.designer_controls(ui, trace_viewport_class(viewport_class), ui_pass);
                    if !self.show_resources && self.designer_mode == DesignerMode::Design {
                        self.basic_authoring_toolbar(
                            ui,
                            &frame.feature_defaults,
                            trace_viewport_class(viewport_class),
                            ui_pass,
                        );
                    }
                    self.pending_drop_ui(ui);
                    let pane = designer_pane_layout(
                        ui.available_size(),
                        ui.spacing().item_spacing,
                        self.tree_visible,
                        self.inspector_visible,
                        self.tree_width,
                        self.inspector_width,
                    );
                    if self.show_resources {
                        // Skins/assets are the main bounded content in Skins
                        // mode.  They replace the full-height canvas rather
                        // than being appended below it, so every control is
                        // reachable through this internal scroll region at
                        // compact viewport sizes.
                        show_designer_scroll_area(
                            ui,
                            egui::ScrollArea::vertical().id_salt("radial-designer-resources"),
                            DesignerAuthoringScrollOwner::Resources,
                            trace_viewport_class(viewport_class),
                            trace_correlation(self.session.as_ref()),
                            |ui| {
                                self.resources_ui(
                                    ui,
                                    trace_viewport_class(viewport_class),
                                    ui_pass,
                                );
                                egui::CollapsingHeader::new("Current menu appearance preview")
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(ui.available_width(), 240.0),
                                            egui::Layout::top_down(egui::Align::Min),
                                            |ui| {
                                                let mut navigation = None;
                                                self.preview.ui(
                                                    ui,
                                                    &draft,
                                                    generation,
                                                    0.65,
                                                    PreviewPreset::Current,
                                                    preview_selection.as_ref(),
                                                    prepared_preview.as_deref(),
                                                    editor_session,
                                                    show_expected_layout_diagnostics,
                                                    DesignerMode::Design,
                                                    None,
                                                    &mut self.projected_selection,
                                                    &mut self.drag_payload,
                                                    &mut self.placement_draft,
                                                    &mut self.pending_drop,
                                                    &mut navigation,
                                                    &mut self.visited_path,
                                                    &mut self.canvas_pan,
                                                    &mut self.pan_drag_start,
                                                );
                                            },
                                        );
                                    });
                            },
                        );
                    } else {
                        let correlation = trace_correlation(self.session.as_ref());
                        ui.horizontal(|ui| {
                            if pane.tree_width > 0.0 {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(pane.tree_width, pane.height),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        self.tree(
                                            ui,
                                            &frame.feature_defaults,
                                            trace_viewport_class(viewport_class),
                                            ui_pass,
                                        );
                                    },
                                );
                                ui.add_space(DESIGNER_PANE_GAP);
                            }
                            let canvas_response = ui.allocate_ui_with_layout(
                                egui::vec2(pane.canvas_width, pane.height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    let mut navigation_intent = None;
                                    self.preview.ui(
                                        ui,
                                        &draft,
                                        generation,
                                        self.preview_zoom,
                                        self.preview_preset,
                                        preview_selection.as_ref(),
                                        prepared_preview.as_deref(),
                                        editor_session,
                                        show_expected_layout_diagnostics,
                                        self.designer_mode,
                                        self.session.as_mut(),
                                        &mut self.projected_selection,
                                        &mut self.drag_payload,
                                        &mut self.placement_draft,
                                        &mut self.pending_drop,
                                        &mut navigation_intent,
                                        &mut self.visited_path,
                                        &mut self.canvas_pan,
                                        &mut self.pan_drag_start,
                                    );
                                    if let (Some(intent), Some(session)) =
                                        (navigation_intent, self.session.as_ref())
                                    {
                                        queue_scope_change(
                                            &mut self.pending_scope_change,
                                            session,
                                            ScopeChangeIntent::Navigation(intent),
                                        );
                                    }
                                },
                            );
                            trace_designer_authoring_control(
                                ui,
                                &canvas_response.response,
                                DesignerAuthoringTarget::Canvas,
                                DesignerAuthoringRole::Region,
                                None,
                                true,
                                false,
                                trace_viewport_class(viewport_class),
                                correlation,
                            );
                            trace_designer_canvas_allocation(
                                ui,
                                &canvas_response.response,
                                egui::vec2(pane.canvas_width, pane.height),
                                correlation,
                            );
                            if pane.inspector_width > 0.0 {
                                ui.add_space(DESIGNER_PANE_GAP);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(pane.inspector_width, pane.height),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        show_designer_scroll_area(
                                            ui,
                                            egui::ScrollArea::vertical()
                                                .id_salt("radial-designer-inspector"),
                                            DesignerAuthoringScrollOwner::Inspector,
                                            trace_viewport_class(viewport_class),
                                            trace_correlation(self.session.as_ref()),
                                            |ui| {
                                                self.inspector(
                                                    ui,
                                                    frame,
                                                    trace_viewport_class(viewport_class),
                                                    ui_pass,
                                                )
                                            },
                                        );
                                    },
                                );
                            }
                        });
                    }
                },
            );
        };
        // `show_deferred` handles the embedded class with a bounded status
        // message before this body can be reached.  The authoritative editor
        // is therefore rendered only by the independent native viewport.
        let parent_native_ui_owner_current = native_ui_owner_current
            && binding
                .is_some_and(|binding| matches!(binding.owner, NavigationOwner::Dialog { .. }));
        egui::CentralPanel::default().show(ui, |ui| {
            if parent_native_ui_owner_current {
                radial_ui_pass.commit_allowed =
                    first_ui_pass && ui.is_visible() && !ui.is_sizing_pass();
                designer_body(ui, &mut radial_ui_pass);
            } else {
                let mut parent_pass = RadialUiPass::default();
                parent_pass.native_ui_allowed = false;
                parent_pass.commit_allowed = false;
                radial_ui_pass.commit_allowed = false;
                ui.add_enabled_ui(false, |ui| designer_body(ui, &mut parent_pass));
            }
        });
        if self.preferences.expanded_sections != expanded_sections_before {
            self.mark_preferences_changed();
        }
        self.properties_popup_ui(&ctx, frame, &mut radial_ui_pass);
        self.radial_add_to_menu_ui_for_instance(
            &ctx,
            frame.navigation_instance,
            &mut radial_ui_pass,
        );
        if !ctx.will_discard() {
            self.native_combo_anchors = std::mem::take(&mut radial_ui_pass.combo_anchors);
        }
        if self.preferences.zoom != self.preview_zoom
            || self.preferences.pan != (self.canvas_pan.x, self.canvas_pan.y)
        {
            self.mark_preferences_changed();
        }
        if native_ui_owner_current
            && radial_ui_pass.commit_allowed
            && !self
                .session
                .as_ref()
                .is_some_and(RadialAuthoringSession::is_initial_snapshot_pending)
        {
            self.apply_post_render();
        }
        self.prompts(&ctx, frame.navigation_instance, &mut radial_ui_pass);
        if parent_native_ui_owner_current && !egui::Popup::is_any_open(&ctx) {
            self.keyboard_shortcuts(&ctx);
        }
        self.trace_designer_submission();
        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            let size = (rect.width(), rect.height());
            if size.0.is_finite() && size.1.is_finite() && size.0 > 0.0 && size.1 > 0.0 {
                let size = (size.0, size.1);
                if self.preferences.window_size != size {
                    self.preferences.window_size = size;
                    self.mark_preferences_changed();
                }
            }
        }
        if viewport_class != egui::ViewportClass::EmbeddedWindow
            && let Some(rect) = ctx.input(|input| input.viewport().outer_rect)
        {
            let position = (rect.min.x, rect.min.y);
            let scale_factor = ctx.pixels_per_point();
            if position.0.is_finite()
                && position.1.is_finite()
                && scale_factor.is_finite()
                && (crate::settings::RadialDesignerPreferences::MIN_WINDOW_SCALE_FACTOR
                    ..=crate::settings::RadialDesignerPreferences::MAX_WINDOW_SCALE_FACTOR)
                    .contains(&scale_factor)
            {
                let position_changed = self.preferences.window_position != Some(position);
                let scale_changed = self.preferences.window_scale_factor != Some(scale_factor);
                if position_changed {
                    self.preferences.window_position = Some(position);
                }
                if scale_changed {
                    self.preferences.window_scale_factor = Some(scale_factor);
                }
                if position_changed || scale_changed {
                    self.mark_preferences_changed();
                }
            }
        }
        let selection_after = self
            .session
            .as_ref()
            .and_then(|session| session.selection.clone());
        if selection_after != selection_before {
            self.focus_restore = selection_after;
        }
        self.enqueue_preferences_ready_if_due();
        self.trace_authoring_geometry(Some(&frame.action_catalog));
        if acceptance_trace::enabled()
            && let Some(session) = self.session.as_ref()
        {
            let state = self.appearance.observation(
                session,
                prepared_preview.as_ref(),
                prepared_menu.as_ref(),
                prepared_appearance,
            );
            acceptance_trace::emit_appearance_state(state);
        }
        if self.preferences_dirty && !self.preferences_flush_requested {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
        self.native_jump_mode.finish_ui_pass();
        let after_owner = binding.map_or(NavigationOwner::Blocked, |binding| {
            self.native_jump_mode.update_navigation_owner_for_viewport(
                &ctx,
                binding,
                frame.application_input_allowed && frame.jump_mode_enabled,
            )
        });
        if let Some(binding) = binding {
            if self.native_jump_mode.prepare_pass_for_viewport(
                &ctx,
                binding,
                after_owner,
                frame.application_input_allowed,
            ) {
                crate::gui::jump_mode::paint::paint(&ctx, &self.native_jump_mode);
            }
            if !ctx.will_discard() {
                self.native_jump_mode.complete_frame_for_viewport(
                    &ctx,
                    binding,
                    after_owner,
                    frame.application_input_allowed,
                );
            }
        } else {
            self.native_jump_mode.cancel_all();
        }
    }

    /// Record the bounded scalar edge at which the Designer submits a changed
    /// session/generation to its normal render/update path.  This is not a
    /// backend presentation acknowledgement, so the event is intentionally
    /// named `DesignerSubmitted` and is emitted for keyboard/programmatic
    /// mutations as well as pointer-driven changes.
    fn trace_designer_submission(&mut self) {
        if !acceptance_trace::enabled() {
            return;
        }
        let Some(session) = self.session.as_ref() else {
            mark_trace_submission(&mut self.trace_submission, None);
            return;
        };
        let key = (session.editor_session.0, session.generation.0);
        if !mark_trace_submission(&mut self.trace_submission, Some(key)) {
            return;
        }
        let mut correlation = trace_correlation(Some(session));
        correlation.session_id = key.0;
        correlation.generation = key.1;
        acceptance_trace::emit(Event::DesignerSubmitted { correlation });
    }

    fn conflict_ui(&mut self, ui: &mut egui::Ui, reason: &str) {
        ui.group(|ui| {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Radial configuration changed externally",
            );
            ui.label(reason);
            secondary_help(
                ui,
                "Choose how to reconcile the open draft before continuing to edit.",
            );
            ui.horizontal_wrapped(|ui| {
                if ui.button("Reload external").clicked() {
                    self.resolve_conflict(ConflictResolution::Reload);
                }
                let discard = ui.button("Discard draft");
                if with_button_emphasis(ui, discard, ButtonEmphasis::Destructive).clicked() {
                    self.resolve_conflict(ConflictResolution::DiscardDraft);
                }
                if ui.button("Rebase local draft").clicked() {
                    self.resolve_conflict(ConflictResolution::Rebase);
                }
            });
        });
    }

    fn properties_popup_ui(
        &mut self,
        ctx: &egui::Context,
        frame: &DesignerFrameContext,
        ui_pass: &mut RadialUiPass,
    ) {
        if self.pending_scope_change.is_some()
            && !self.properties_popup_dirty()
            && !self.inspector_action_editor.has_unassigned_text()
        {
            if let Some(pending) = self.pending_scope_change.take() {
                self.apply_pending_scope_change(pending);
            }
            return;
        }
        let Some(StableSelection::Cell {
            menu_id,
            ring_id,
            cell_id,
        }) = self.properties_popup.clone()
        else {
            self.inspector_scope_change_guard_ui(ctx, frame.navigation_instance, ui_pass);
            return;
        };
        let cell = self
            .session
            .as_ref()
            .and_then(|session| {
                session
                    .draft
                    .menus
                    .iter()
                    .find(|menu| menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
                    .and_then(|ring| ring.cells.iter().find(|cell| cell.id == cell_id))
            })
            .cloned();
        let Some(cell) = cell else {
            self.properties_popup = None;
            self.properties_draft = None;
            return;
        };
        let target = StableSelection::Cell {
            menu_id: menu_id.clone(),
            ring_id: ring_id.clone(),
            cell_id: cell_id.clone(),
        };
        let generation = self
            .session
            .as_ref()
            .map(|session| session.generation)
            .unwrap_or_default();
        // A draft is initialized exactly once for a target.  If the service
        // advances the generation while the popup is open, retain the edits
        // and let Apply reject the stale generation instead of silently
        // replacing them with a newer snapshot.
        if self
            .properties_draft
            .as_ref()
            .is_none_or(|draft| draft.target != target)
        {
            self.properties_draft = Some(PropertiesDraft::from_cell(
                target.clone(),
                generation,
                &cell,
            ));
        }
        let inspector_dirty = self.inspector_action_editor.has_unassigned_text();
        let inspector_buffer = self.pending_inspector_action_buffer_for_target(&target);
        let has_pending_scope_change = self.pending_scope_change.is_some();
        let pending_scope_change_current = self
            .pending_scope_change
            .as_ref()
            .is_some_and(|pending| self.scope_change_is_current(pending));
        let Some(mut draft) = self.properties_draft.clone() else {
            return;
        };
        let mut apply = false;
        let mut open_inspector_after_apply = false;
        let mut discard_and_open = false;
        let mut cancel = false;
        let mut navigate_after_apply = false;
        let mut popup_open = true;
        let mut handoff_prompt = self.properties_handoff_prompt;
        let (popup_window_id, popup_native_owner) = self.radial_child_window_config(
            ui_pass,
            frame.navigation_instance,
            RadialDesignerChildKind::CellProperties,
            radial_selection_digest(&target),
            egui::Id::new(("radial-designer-cell-popup", &cell_id)),
        );
        let mut popup_pass = RadialUiPass {
            jump: popup_native_owner.then(|| ui_pass.jump.clone()).flatten(),
            combo_anchors: Vec::new(),
            owner: if popup_native_owner {
                ui_pass.owner
            } else {
                NavigationOwner::Blocked
            },
            native_ui_allowed: popup_native_owner && ui_pass.native_ui_allowed,
            commit_allowed: popup_native_owner && ui_pass.commit_allowed,
        };
        let mut popup_visible_commit = false;
        let titlebar_commit = popup_native_owner && ctx.current_pass_index() == 0;
        let invocation_context = self
            .session
            .as_ref()
            .and_then(|session| session.sampled_preview_context.clone())
            .unwrap_or_else(|| InvocationContext::empty(0));
        let correlation = trace_correlation(self.session.as_ref());
        let Some(editor_session) = self.session.as_ref().map(|session| session.editor_session)
        else {
            return;
        };
        let mut binding_editor_intents = Vec::new();
        let submenu_choices = self
            .session
            .as_ref()
            .map(|session| {
                session
                    .draft
                    .menus
                    .iter()
                    .filter(|menu| menu.id != menu_id)
                    .map(|menu| (menu.id.clone(), menu.name.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        egui::Window::new("Cell properties")
            .id(popup_window_id)
            .collapsible(false)
            .resizable(false)
            .default_width(300.0)
            .open(&mut popup_open)
            .show(ctx, |ui| {
                popup_visible_commit = popup_native_owner
                    && ctx.current_pass_index() == 0
                    && ui.is_visible()
                    && !ui.is_sizing_pass();
                popup_pass.commit_allowed = popup_visible_commit;
                ui.add_enabled_ui(popup_native_owner, |ui| {
                compact_section_heading(ui, "Cell properties", None);
                if self.radial_add_request.is_some() {
                    secondary_help(
                        ui,
                        "Add to radial is waiting. Apply or cancel these staged properties first; the add request will resume afterward.",
                    );
                }
                let label_response = ui.add(egui::TextEdit::singleline(&mut draft.label).hint_text("Label"));
                popup_pass.register(ui, &label_response, "cell-properties/label", "Cell label", TargetKind::TextEntry);
                let selected_type = [
                    ("Spacer", 0),
                    ("Action", 1),
                    ("Submenu", 2),
                    ("Dynamic source", 3),
                    ("Control", 4),
                ]
                .iter()
                .find(|(_, kind)| *kind == draft.content_kind)
                .map_or("Unknown", |(label, _)| *label);
                let type_combo_identity = format!(
                    "cell-properties/type/{:016x}",
                    ui.make_persistent_id("Type").value()
                );
                let type_combo = egui::ComboBox::from_label("Type")
                    .selected_text(selected_type)
                    .show_ui(ui, |ui| {
                        for (label, kind) in [
                            ("Spacer", 0),
                            ("Action", 1),
                            ("Submenu", 2),
                            ("Dynamic source", 3),
                            ("Control", 4),
                        ] {
                            let response = ui.scope_builder(
                                egui::UiBuilder::new().id(ui.id().with(("cell-property-type", kind))),
                                |ui| ui.selectable_value(&mut draft.content_kind, kind, label),
                            ).inner;
                            popup_pass.register_combo_option(
                                ui,
                                &response,
                                &type_combo_identity,
                                &format!("cell-properties/type/{kind}"),
                                "Cell type",
                            );
                            if kind == 1 {
                                trace_designer_authoring_control(
                                    ui,
                                    &response,
                                    DesignerAuthoringTarget::ActionTypeOption,
                                    DesignerAuthoringRole::Selectable,
                                    None,
                                    true,
                                    draft.content_kind == kind,
                                    ViewportClass::Deferred,
                                    correlation,
                                );
                            }
                        }
                    });
                popup_pass.register_combo(ui, &type_combo.response, &type_combo_identity, "Cell type");
                trace_designer_authoring_control(
                    ui,
                    &type_combo.response,
                    DesignerAuthoringTarget::CellType,
                    DesignerAuthoringRole::ComboBox,
                    None,
                    true,
                    false,
                    ViewportClass::Deferred,
                    correlation,
                );
                if draft.content_kind == 3 {
                    enum_combo(
                        ui,
                        "Dynamic source",
                        &mut draft.dynamic_source,
                        &[
                            ("Favorites", 0),
                            ("Recent", 1),
                            ("Clipboard", 2),
                            ("Snippets", 3),
                            ("Notes", 4),
                            ("Windows", 5),
                            ("Macros", 6),
                            ("Applications", 7),
                            ("Dashboard", 8),
                            ("Launcher results", 9),
                            ("Launcher query", 10),
                        ],
                        &mut popup_pass);
                }
                if draft.content_kind != 1 {
                    draft.action_editor.invalidate_visit();
                }
                if draft.content_kind == 1 {
                    let scope = BindingEditorScope {
                        surface: BindingEditorSurface::Properties,
                        editor_session,
                        draft_generation: draft.generation,
                        target: target.clone(),
                        slot: BindingEditorSlot::CellPrimary,
                    };
                    let assigned_binding = draft.action_binding.clone();
                    binding_editor_intents.extend(draft.action_editor.show_with_action_catalog_and_jump(
                        ui,
                        scope,
                        assigned_binding.as_ref(),
                        &invocation_context,
                        Some(frame.action_catalog.as_ref()),
                        Some(&popup_pass),
                    ));
                }
                if draft.content_kind == 2 {
                    let submenu_combo_identity = format!(
                        "cell-properties/submenu/{:016x}",
                        ui.make_persistent_id("Submenu").value()
                    );
                    let submenu_combo = egui::ComboBox::from_label("Submenu")
                        .selected_text(
                            draft
                                .submenu_target
                                .as_ref()
                                .and_then(|target| {
                                    submenu_choices
                                        .iter()
                                        .find(|(id, _)| id == target)
                                        .map(|(_, name)| name.clone())
                                })
                                .unwrap_or_else(|| "Choose menu".into()),
                        )
                        .show_ui(ui, |ui| {
                            for (id, name) in &submenu_choices {
                                let response = ui.selectable_value(
                                    &mut draft.submenu_target,
                                    Some(id.clone()),
                                    name,
                                );
                                popup_pass.register_combo_option(
                                    ui,
                                    &response,
                                    &submenu_combo_identity,
                                    &format!("cell-properties/submenu/{:016x}", radial_value_digest(id)),
                                    "Submenu choice",
                                );
                            }
                        });
                    popup_pass.register_combo(ui, &submenu_combo.response, &submenu_combo_identity, "Submenu choice");
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("Icon");
                    for (identity, value, label) in [("inherit", 0, "Inherit"), ("clear", 1, "Clear"), ("keep", 2, "Keep value")] {
                        let response = ui.selectable_value(&mut draft.icon_kind, value, label);
                        popup_pass.register(ui, &response, &format!("cell-properties/icon/{identity}"), "Icon option", TargetKind::Selectable);
                    }
                });
                secondary_help(
                    ui,
                    "Action bindings and advanced appearance options are available in Inspector.",
                );
                if draft.action_editor.has_unassigned_text() {
                    secondary_help(
                        ui,
                        "Save query or Use exact command before applying, or discard the pending action input.",
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    let can_apply_popup = !draft.action_editor.has_unassigned_text();
                    let button = ui.add_enabled(
                        can_apply_popup,
                        egui::Button::new("Apply"),
                    );
                    let response = with_button_emphasis(ui, button, ButtonEmphasis::Primary);
                    popup_pass.register(ui, &response, "cell-properties/apply", "Apply cell properties", TargetKind::Button);
                    trace_designer_authoring_control(
                        ui,
                        &response,
                        DesignerAuthoringTarget::PopupApply,
                        DesignerAuthoringRole::Button,
                        None,
                        can_apply_popup,
                        false,
                        ViewportClass::Deferred,
                        correlation,
                    );
                    if response.clicked() && can_apply_popup && !has_pending_scope_change {
                        apply = true;
                    }
                    let response = ui.button("Cancel");
                    popup_pass.register(ui, &response, "cell-properties/cancel", "Cancel cell properties", TargetKind::Button);
                    trace_designer_authoring_control(
                        ui,
                        &response,
                        DesignerAuthoringTarget::PopupCancel,
                        DesignerAuthoringRole::Button,
                        None,
                        true,
                        false,
                        ViewportClass::Deferred,
                        correlation,
                    );
                    if response.clicked() && !has_pending_scope_change {
                        cancel = true;
                    }
                    let response = ui.button("Open in Inspector");
                    popup_pass.register(ui, &response, "cell-properties/open-inspector", "Open Inspector", TargetKind::Button);
                    trace_designer_authoring_control(
                        ui,
                        &response,
                        DesignerAuthoringTarget::PopupOpenInspector,
                        DesignerAuthoringRole::Button,
                        None,
                        true,
                        false,
                        ViewportClass::Deferred,
                        correlation,
                    );
                    if response.clicked() && !has_pending_scope_change {
                        if draft.differs_from(&cell) {
                            handoff_prompt = true;
                        } else {
                            discard_and_open = true;
                        }
                    }
                });
                if handoff_prompt && !has_pending_scope_change {
                    ui.separator();
                    ui.label("Apply popup edits before opening Inspector?");
                    ui.horizontal_wrapped(|ui| {
                        let can_apply_popup = !draft.action_editor.has_unassigned_text();
                        let button = ui.add_enabled(
                            can_apply_popup,
                            egui::Button::new("Apply and open"),
                        );
                        let response =
                            with_button_emphasis(ui, button, ButtonEmphasis::Primary);
                        popup_pass.register(ui, &response, "cell-properties/apply-and-open", "Apply and open Inspector", TargetKind::Button);
                        trace_designer_authoring_control(
                            ui,
                            &response,
                            DesignerAuthoringTarget::PopupApplyAndOpen,
                            DesignerAuthoringRole::Button,
                            None,
                            can_apply_popup,
                            false,
                            ViewportClass::Deferred,
                            correlation,
                        );
                        if response.clicked() && can_apply_popup {
                            apply = true;
                            open_inspector_after_apply = true;
                            handoff_prompt = false;
                        }
                        let button = ui.button("Discard and open");
                        let response =
                            with_button_emphasis(ui, button, ButtonEmphasis::Destructive);
                        popup_pass.register(ui, &response, "cell-properties/discard-and-open", "Discard and open Inspector", TargetKind::Button);
                        trace_designer_authoring_control(
                            ui,
                            &response,
                            DesignerAuthoringTarget::PopupDiscardAndOpen,
                            DesignerAuthoringRole::Button,
                            None,
                            true,
                            false,
                            ViewportClass::Deferred,
                            correlation,
                        );
                        if response.clicked() {
                            discard_and_open = true;
                            handoff_prompt = false;
                        }
                        let response = ui.button("Keep editing");
                        popup_pass.register(ui, &response, "cell-properties/keep-editing", "Keep editing", TargetKind::Button);
                        trace_designer_authoring_control(
                            ui,
                            &response,
                            DesignerAuthoringTarget::PopupKeepEditing,
                            DesignerAuthoringRole::Button,
                            None,
                            true,
                            false,
                            ViewportClass::Deferred,
                            correlation,
                        );
                        if response.clicked() {
                            handoff_prompt = false;
                        }
                    });
                }
            });
            });
        ui_pass.combo_anchors.extend(popup_pass.combo_anchors);
        let titlebar_close = titlebar_commit && !popup_open;
        if titlebar_close {
            popup_open = false;
        }
        // Apply the editor's authorized output to the local draft before the
        // single owner-visible writeback below. The navigation guard and an
        // explicit Apply in this same pass must observe the exact assignment
        // that Pin, Save query, or Use exact command just accepted.
        for intent in binding_editor_intents {
            match intent {
                ActionBindingEditorIntent::Pin { binding } => {
                    draft.action_binding = Some(binding.clone());
                    draft
                        .action_editor
                        .accept_committed_assignment(binding, BindingAssignmentKind::Pinned);
                }
                ActionBindingEditorIntent::SaveQuery { binding }
                | ActionBindingEditorIntent::SetCommand { binding } => {
                    draft.action_binding = Some(binding.clone());
                    draft
                        .action_editor
                        .accept_committed_assignment(binding, BindingAssignmentKind::Explicit);
                }
                ActionBindingEditorIntent::Search { identity, query } => {
                    frame
                        .intent_bridge
                        .push(DesignerUiIntent::ActionSearch { identity, query });
                }
                ActionBindingEditorIntent::Test {
                    identity,
                    binding,
                    history_query,
                } => frame.intent_bridge.push(DesignerUiIntent::TestAction {
                    identity,
                    payload: Box::new(crate::gui::AuthoringProviderTestPayload {
                        binding,
                        invocation: invocation_context.clone(),
                        history_query,
                    }),
                }),
            }
        }
        if popup_visible_commit {
            self.properties_draft = Some(draft.clone());
            self.properties_handoff_prompt = handoff_prompt;
        } else {
            // The parent popup may still paint underneath an exact child
            // confirmation. Keep rendering it disabled, but derive any guard
            // state from the last committed draft rather than the shadow UI.
            if let Some(committed_draft) = self.properties_draft.clone() {
                draft = committed_draft;
            }
        }
        let properties_action_pending = draft.action_editor.has_unassigned_text();
        let navigation_dirty = draft.differs_from(&cell)
            || draft.action_editor.has_unassigned_text()
            || inspector_dirty;
        let can_apply_navigation = pending_scope_change_current
            && !properties_action_pending
            && (!inspector_dirty || inspector_buffer.is_some());
        let mut navigation_decision = None;
        if has_pending_scope_change && navigation_dirty {
            let guard_subject = self.pending_scope_change.as_ref().map_or(0, |pending| {
                radial_value_digest(&(
                    pending.anchor.editor_session,
                    pending.anchor.generation,
                    pending.anchor.selection.as_ref(),
                    std::mem::discriminant(&pending.intent),
                ))
            });
            let (guard_window_id, guard_native_owner) = self.radial_child_window_config(
                ui_pass,
                frame.navigation_instance,
                RadialDesignerChildKind::ScopeChangeGuard,
                guard_subject,
                egui::Id::new("radial-designer-navigation-guard"),
            );
            let mut guard_open = true;
            let mut guard_visible_commit = false;
            egui::Window::new("Unsaved cell properties")
                .id(guard_window_id)
                .collapsible(false)
                .resizable(false)
                .open(&mut guard_open)
                .show(ctx, |ui| {
                    guard_visible_commit = guard_native_owner
                        && ctx.current_pass_index() == 0
                        && ui.is_visible()
                        && !ui.is_sizing_pass();
                    ui_pass.commit_allowed = guard_visible_commit;
                    if !guard_native_owner {
                        ui.disable();
                    }
                    ui.label(if pending_scope_change_current {
                        "Apply or discard the current edits before continuing."
                    } else {
                        "The requested change is stale. Cancel it and review the current draft."
                    });
                    if properties_action_pending {
                        secondary_help(
                            ui,
                            "Save query or Use exact command before applying; Discard and continue abandons this action input.",
                        );
                    }
                    ui.horizontal_wrapped(|ui| {
                        let apply_only = self
                            .pending_scope_change
                            .as_ref()
                            .is_some_and(|pending| {
                                !pending.intent.apply_and_continue_is_safe()
                            });
                        let apply = ui
                            .add_enabled(
                                can_apply_navigation,
                                egui::Button::new(if apply_only {
                                    "Apply edits"
                                } else {
                                    "Apply and continue"
                                }),
                            );
                        let apply = with_button_emphasis(ui, apply, ButtonEmphasis::Primary);
                        ui_pass.register(ui, &apply, "scope-change/apply", "Apply property changes", TargetKind::Button);
                        if apply.clicked() && guard_visible_commit {
                            navigation_decision = Some(PropertiesNavigationDecision::Apply);
                        }
                        let discard = ui.button(if pending_scope_change_current {
                                "Discard and continue"
                            } else {
                                "Discard edits and cancel stale change"
                            });
                        let discard = with_button_emphasis(ui, discard, ButtonEmphasis::Destructive);
                        ui_pass.register(ui, &discard, "scope-change/discard", "Discard property changes", TargetKind::Button);
                        if discard.clicked() && guard_visible_commit {
                            navigation_decision = Some(PropertiesNavigationDecision::Discard);
                        }
                        let cancel = ui.button("Cancel navigation");
                        ui_pass.register(ui, &cancel, "scope-change/cancel", "Cancel navigation", TargetKind::Button);
                        if cancel.clicked() && guard_visible_commit {
                            navigation_decision = Some(PropertiesNavigationDecision::Cancel);
                        }
                    });
                });
            if !guard_native_owner || (!guard_visible_commit && guard_open) {
                navigation_decision = None;
            } else if !guard_open {
                navigation_decision = Some(PropertiesNavigationDecision::Cancel);
            }
        }
        match navigation_decision {
            Some(PropertiesNavigationDecision::Cancel) => {
                self.pending_scope_change = None;
                self.properties_handoff_prompt = false;
                return;
            }
            Some(PropertiesNavigationDecision::Discard) => {
                if let Some(draft) = self.properties_draft.as_mut() {
                    draft.action_editor.invalidate_pending_requests();
                    draft.action_editor.invalidate_visit();
                }
                self.discard_inspector_action_buffer();
                let pending = self.pending_scope_change.take();
                self.properties_popup = None;
                self.properties_draft = None;
                self.properties_handoff_prompt = false;
                if pending_scope_change_current && let Some(pending) = pending {
                    self.apply_pending_scope_change(pending);
                }
                return;
            }
            Some(PropertiesNavigationDecision::Apply) => {
                apply = true;
                navigate_after_apply = true;
            }
            None => {}
        }
        if !popup_native_owner && !apply {
            return;
        }
        if !popup_visible_commit && !titlebar_close && !apply {
            return;
        }
        if !popup_open {
            if has_pending_scope_change {
                // Window close while navigation is pending follows the same
                // explicit discard guard as an in-editor navigation request.
                return;
            }
            cancel = true;
        }
        if cancel {
            self.invalidate_binding_editor_scope();
            self.properties_popup = None;
            self.properties_draft = None;
            self.properties_handoff_prompt = false;
            return;
        }
        if discard_and_open {
            self.invalidate_binding_editor_scope();
            if let Some(session) = self.session.as_mut() {
                session.select(Some(target));
            }
            self.inspector_visible = true;
            self.properties_popup = None;
            self.properties_draft = None;
            self.mark_preferences_changed();
            return;
        }
        if apply {
            let draft = self
                .properties_draft
                .clone()
                .expect("draft initialized above");
            let inspector_buffer = self.pending_inspector_action_buffer_for_target(&target);
            if self.inspector_action_editor.has_unassigned_text() && inspector_buffer.is_none() {
                self.resource_notice = Some(ResourceNotice::warning(
                    "Inspector action input is stale or incomplete; keep editing or discard it",
                ));
                return;
            }
            let mut content = match draft.content_kind {
                0 => CellContent::Spacer,
                1 => match &draft.original_content {
                    CellContent::Action { .. } => draft
                        .action_binding
                        .map(|binding| CellContent::Action { binding })
                        .unwrap_or_else(|| draft.original_content.clone()),
                    _ => {
                        let Some(binding) = draft.action_binding else {
                            self.resource_notice = Some(ResourceNotice::warning(
                                "Choose an action before applying this type",
                            ));
                            return;
                        };
                        CellContent::Action { binding }
                    }
                },
                2 => {
                    let Some(menu_id) = draft.submenu_target.clone() else {
                        self.resource_notice = Some(ResourceNotice::warning(
                            "Choose a submenu before applying this type",
                        ));
                        return;
                    };
                    CellContent::Submenu { menu_id }
                }
                3 => CellContent::Dynamic {
                    source: dynamic_source_for_key(draft.dynamic_source),
                },
                _ => CellContent::Control {
                    control: match &draft.original_content {
                        CellContent::Control { control } => *control,
                        _ => Control::Back,
                    },
                },
            };
            if let Some((_, binding)) = &inspector_buffer {
                content = CellContent::Action {
                    binding: binding.clone(),
                };
            }
            let Some(session) = self.session.as_mut() else {
                return;
            };
            if session.generation != draft.generation {
                self.resource_notice = Some(ResourceNotice::warning(
                    "Cell properties draft is stale; reopen it before applying",
                ));
                return;
            }
            let mut document = (*session.draft).clone();
            if let Some(slot) = document
                .menus
                .iter_mut()
                .find(|menu| menu.id == menu_id)
                .and_then(|menu| menu.rings.iter_mut().find(|ring| ring.id == ring_id))
                .and_then(|ring| ring.cells.iter_mut().find(|cell| cell.id == cell_id))
            {
                slot.label = draft.label;
                slot.content = content;
                slot.icon = match draft.icon_kind {
                    0 => crate::radial::model::Override::Inherit,
                    1 => crate::radial::model::Override::Clear,
                    _ => draft.original_icon,
                };
                if let Err(error) = menu::validate_submenu_graph(&document) {
                    self.resource_notice = Some(ResourceNotice::error(format!(
                        "Cell properties rejected: {error:?}"
                    )));
                } else if let Err(error) = session.replace_document_atomic(document) {
                    self.resource_notice = Some(ResourceNotice::error(format!(
                        "Cell properties failed: {error:?}"
                    )));
                } else {
                    if let Some((_, binding)) = inspector_buffer {
                        self.inspector_action_editor
                            .set_assigned_binding(Some(binding));
                    }
                    if open_inspector_after_apply {
                        session.select(Some(target));
                        self.inspector_visible = true;
                        self.mark_preferences_changed();
                    }
                    self.properties_popup = None;
                    self.properties_draft = None;
                    self.placement_draft = None;
                    if navigate_after_apply
                        && let Some(mut pending) = self.pending_scope_change.take()
                    {
                        if pending.intent.apply_and_continue_is_safe() {
                            self.rebase_scope_change_after_explicit_apply(&mut pending);
                            self.apply_pending_scope_change(pending);
                        } else {
                            self.resource_notice = Some(ResourceNotice::info(
                                "The action was applied. Repeat the requested change to confirm it.",
                            ));
                        }
                    }
                }
            }
        }
    }

    fn resolve_conflict(&mut self, resolution: ConflictResolution) {
        self.queue_editor_operation(DeferredEditorOperation::ResolveConflict(resolution));
    }

    fn designer_controls(
        &mut self,
        ui: &mut egui::Ui,
        viewport: ViewportClass,
        ui_pass: &RadialUiPass,
    ) {
        let jump = ui_pass.child_jump("designer-controls");
        ui.horizontal_wrapped(|ui| {
            ui.label("Mode:");
            let design = ui.selectable_label(self.designer_mode == DesignerMode::Design, "Design");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &design,
                    "mode/design",
                    "Design mode",
                    TargetKind::Button,
                );
            }
            if design.clicked() {
                self.designer_mode = DesignerMode::Design;
                self.preview.cancel_tooltip();
                self.mark_preferences_changed();
            }
            let preview = ui.selectable_label(
                self.designer_mode == DesignerMode::PreviewTest,
                "Preview / Test",
            );
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &preview,
                    "mode/preview",
                    "Preview and test mode",
                    TargetKind::Button,
                );
            }
            if preview.clicked() {
                self.designer_mode = DesignerMode::PreviewTest;
                self.preview.cancel_tooltip();
                self.mark_preferences_changed();
            }
            ui.separator();
            let fit = ui.button("Fit");
            if let Some(jump) = jump.as_ref() {
                jump.register(ui, &fit, "fit", "Fit preview", TargetKind::Button);
            }
            trace_pointer_release_response(
                ui,
                &fit,
                WidgetCategory::Zoom,
                fit.clicked(),
                trace_correlation(self.session.as_ref()),
            );
            if fit.clicked() {
                self.preview_zoom = 1.0;
                self.canvas_pan = CanvasPoint::default();
                self.pan_drag_start = None;
                self.preview.cancel_tooltip();
                self.mark_preferences_changed();
            }
            let menus = ui.selectable_label(!self.show_resources, "Menus");
            if let Some(jump) = jump.as_ref() {
                jump.register(ui, &menus, "view/menus", "Menus view", TargetKind::Button);
            }
            trace_designer_semantic_target(
                ui,
                &menus,
                DesignerSemanticTarget::Menus,
                DesignerSemanticRole::SelectableLabel,
                viewport,
                !self.show_resources,
                trace_correlation(self.session.as_ref()),
            );
            trace_pointer_release_response(
                ui,
                &menus,
                WidgetCategory::Menus,
                menus.clicked(),
                trace_correlation(self.session.as_ref()),
            );
            if menus.clicked() {
                self.show_resources = false;
                self.mark_preferences_changed();
            }
            let skins = ui.selectable_label(self.show_resources, "Skins");
            if let Some(jump) = jump.as_ref() {
                jump.register(ui, &skins, "view/skins", "Skins view", TargetKind::Button);
            }
            trace_designer_semantic_target(
                ui,
                &skins,
                DesignerSemanticTarget::Skins,
                DesignerSemanticRole::SelectableLabel,
                viewport,
                self.show_resources,
                trace_correlation(self.session.as_ref()),
            );
            trace_pointer_release_response(
                ui,
                &skins,
                WidgetCategory::Skins,
                skins.clicked(),
                trace_correlation(self.session.as_ref()),
            );
            if skins.clicked() {
                self.show_resources = true;
                self.mark_preferences_changed();
            }
            let tree = ui
                .selectable_label(self.tree_visible, "Tree")
                .on_hover_text("Show or hide the menu tree");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &tree,
                    "pane/tree",
                    "Show or hide tree",
                    TargetKind::Button,
                );
            }
            trace_designer_semantic_target(
                ui,
                &tree,
                DesignerSemanticTarget::Tree,
                DesignerSemanticRole::SelectableLabel,
                viewport,
                self.tree_visible,
                trace_correlation(self.session.as_ref()),
            );
            trace_pointer_release_response(
                ui,
                &tree,
                WidgetCategory::Tree,
                tree.clicked(),
                trace_correlation(self.session.as_ref()),
            );
            if tree.clicked() {
                self.tree_visible = !self.tree_visible;
                self.mark_preferences_changed();
            }
            let inspector = ui
                .selectable_label(self.inspector_visible, "Inspector")
                .on_hover_text("Show or hide the inspector");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &inspector,
                    "pane/inspector",
                    "Show or hide inspector",
                    TargetKind::Button,
                );
            }
            trace_designer_semantic_target(
                ui,
                &inspector,
                DesignerSemanticTarget::Inspector,
                DesignerSemanticRole::SelectableLabel,
                viewport,
                self.inspector_visible,
                trace_correlation(self.session.as_ref()),
            );
            trace_pointer_release_response(
                ui,
                &inspector,
                WidgetCategory::Inspector,
                inspector.clicked(),
                trace_correlation(self.session.as_ref()),
            );
            if inspector.clicked() {
                self.inspector_visible = !self.inspector_visible;
                self.mark_preferences_changed();
            }
            let layout = ui.menu_button("Layout", |ui| {
                ui.label("Presentation only");
                if self.tree_visible {
                    let response = ui.add(
                        egui::DragValue::new(&mut self.tree_width)
                            .prefix("Tree ")
                            .suffix(" px")
                            .range(120.0..=420.0),
                    );
                    if response.changed() {
                        self.mark_preferences_changed();
                    }
                }
                if self.inspector_visible {
                    let response = ui.add(
                        egui::DragValue::new(&mut self.inspector_width)
                            .prefix("Inspector ")
                            .suffix(" px")
                            .range(220.0..=560.0),
                    );
                    if response.changed() {
                        self.mark_preferences_changed();
                    }
                }
                if ui
                    .button("Reset Designer layout")
                    .on_hover_text("Reset panes, sections, canvas view, and window geometry only")
                    .clicked()
                {
                    self.reset_designer_layout();
                    ui.close();
                }
            });
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &layout.response,
                    "layout-menu",
                    "Layout options",
                    TargetKind::Button,
                );
            }
            ui.small("Direct Design gestures edit the draft only");
        });
    }

    fn pending_drop_ui(&mut self, ui: &mut egui::Ui) {
        let Some(pending) = self.pending_drop.clone() else {
            return;
        };
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Destination occupied");
                ui.small("Choose Swap to exchange both authored slots, or Cancel.");
                if ui.button("Swap").clicked() {
                    self.queue_editor_operation(DeferredEditorOperation::MoveCell {
                        pending: pending.clone(),
                        resolution: menu::CellDropResolution::Swap,
                    });
                }
                if ui.button("Cancel").clicked() {
                    self.pending_drop = None;
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, ui_pass: &RadialUiPass) {
        let jump = ui_pass.child_jump("toolbar");
        ui.horizontal_wrapped(|ui| {
            let pending = self
                .session
                .as_ref()
                .is_some_and(|session| session.pending_request.is_some());
            let undo = ui
                .add_enabled(!pending, egui::Button::new("Undo"))
                .on_hover_text("Undo (Ctrl+Z)");
            if let Some(jump) = jump.as_ref() {
                jump.register(ui, &undo, "undo", "Undo", TargetKind::Button);
            }
            trace_designer_authoring_control(
                ui,
                &undo,
                DesignerAuthoringTarget::Undo,
                DesignerAuthoringRole::Button,
                None,
                !pending,
                false,
                ViewportClass::Deferred,
                trace_correlation(self.session.as_ref()),
            );
            if undo.clicked() {
                self.queue_editor_operation(DeferredEditorOperation::Undo);
            }
            let redo = ui
                .add_enabled(!pending, egui::Button::new("Redo"))
                .on_hover_text("Redo (Ctrl+Shift+Z)");
            if let Some(jump) = jump.as_ref() {
                jump.register(ui, &redo, "redo", "Redo", TargetKind::Button);
            }
            trace_designer_authoring_control(
                ui,
                &redo,
                DesignerAuthoringTarget::Redo,
                DesignerAuthoringRole::Button,
                None,
                !pending,
                false,
                ViewportClass::Deferred,
                trace_correlation(self.session.as_ref()),
            );
            if redo.clicked() {
                self.queue_editor_operation(DeferredEditorOperation::Redo);
            }
            ui.separator();
            let validation = self
                .session
                .as_ref()
                .and_then(|session| crate::radial::validation::validate(&session.draft).err());
            let valid = validation.is_none();
            let apply = ui
                .add_enabled(!pending && valid, egui::Button::new("Apply"))
                .on_disabled_hover_text("Resolve the inline validation diagnostics before Apply");
            let apply = with_button_emphasis(ui, apply, ButtonEmphasis::Primary);
            if let Some(jump) = jump.as_ref() {
                jump.register(ui, &apply, "apply", "Apply draft", TargetKind::Button);
            }
            if apply.clicked() {
                self.send_commit(CommitDisposition::Apply);
            }
            let save = ui
                .add_enabled(!pending && valid, egui::Button::new("Save"))
                .on_disabled_hover_text("Resolve the inline validation diagnostics before Save")
                .on_hover_text("Save and close (Ctrl+S)");
            let save = with_button_emphasis(ui, save, ButtonEmphasis::Primary);
            if let Some(jump) = jump.as_ref() {
                jump.register(ui, &save, "save", "Save draft", TargetKind::Button);
            }
            trace_designer_authoring_control(
                ui,
                &save,
                DesignerAuthoringTarget::Save,
                DesignerAuthoringRole::Button,
                None,
                !pending && valid,
                false,
                ViewportClass::Deferred,
                trace_correlation(self.session.as_ref()),
            );
            if save.clicked() {
                self.send_commit(CommitDisposition::Save);
                self.trace_last_radial_add_operation(RadialInsertionControl::Save);
            }
            let cancel = ui.button("Cancel");
            if let Some(jump) = jump.as_ref() {
                jump.register(ui, &cancel, "cancel", "Cancel editing", TargetKind::Button);
            }
            if cancel.clicked() {
                self.cancel();
            }
        });
        if let Some(session) = &self.session {
            ui.horizontal_wrapped(|ui| {
                if session.is_dirty() {
                    semantic_status(ui, StatusTone::Warning, "Modified", None);
                } else {
                    semantic_status(ui, StatusTone::Success, "Saved", None);
                }
                if let Some(error) = &session.last_error {
                    semantic_status(ui, StatusTone::Error, "Error", Some(error));
                }
            });
        }
        if let Some(errors) = self
            .session
            .as_ref()
            .and_then(|session| crate::radial::validation::validate(&session.draft).err())
        {
            let diagnostics = ui.collapsing(
                format!("{} validation diagnostic(s)", errors.0.len()),
                |ui| {
                    for issue in errors.0 {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!("Error at {}: {}", issue.path, issue.message),
                        );
                    }
                },
            );
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &diagnostics.header_response,
                    "validation-diagnostics",
                    "Validation diagnostics",
                    TargetKind::Button,
                );
            }
        }
    }

    fn trace_last_radial_add_operation(&self, control: RadialInsertionControl) {
        let Some(trace) = self.last_radial_add_trace.as_ref() else {
            return;
        };
        let session = self.session.as_ref();
        let document_digest_after = session.map_or(0, radial_add_document_digest);
        let binding_digest_after = session.map_or(0, |session| {
            radial_add_cell_binding_digest(
                session,
                Some(&trace.menu_id),
                Some(&trace.ring_id),
                Some(&trace.cell_id),
            )
        });
        emit_radial_add_control(
            trace.identity,
            control,
            session,
            Some(&trace.menu_id),
            Some(&trace.ring_id),
            Some(&trace.cell_id),
            true,
            true,
            true,
            document_digest_after,
            binding_digest_after,
        );
    }

    /// Consume editor shortcuts after widgets have had a chance to consume
    /// text-editing keys. This preserves native text undo while still making
    /// the authoring operations keyboard-only accessible.
    fn keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        let pending = self
            .session
            .as_ref()
            .is_some_and(|session| session.pending_request.is_some());
        let modal_open = self.close_prompt
            || self.delete_message.is_some()
            || self.delete_ring_prompt.is_some()
            || self.resize_prompt.is_some();
        if pending || modal_open {
            return;
        }
        let escape =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if escape {
            if self.pending_scope_change.take().is_some() {
                return;
            }
            if !self.tree_filter.is_empty() {
                self.tree_filter.clear();
                return;
            }
            if self.properties_popup.is_some() {
                self.invalidate_binding_editor_scope();
                self.properties_popup = None;
                self.properties_draft = None;
                self.properties_handoff_prompt = false;
                return;
            }
            self.request_close();
            return;
        }
        // TextEdit owns Ctrl+A, Shift selection, and platform text undo. The
        // document history shortcuts are only active when no text widget has
        // claimed keyboard focus.
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (redo, undo, save) = ctx.input_mut(|input| {
            let redo =
                input.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z);
            let undo = !redo && input.consume_key(egui::Modifiers::CTRL, egui::Key::Z);
            let save = input.consume_key(egui::Modifiers::CTRL, egui::Key::S);
            (redo, undo, save)
        });
        if undo {
            self.queue_editor_operation(DeferredEditorOperation::Undo);
        } else if redo {
            self.queue_editor_operation(DeferredEditorOperation::Redo);
        }
        if save
            && self
                .session
                .as_ref()
                .is_some_and(|session| crate::radial::validation::validate(&session.draft).is_ok())
        {
            self.send_commit(CommitDisposition::Save);
        }
    }

    fn preview_controls(
        &mut self,
        ui: &mut egui::Ui,
        show_expected_layout_diagnostics: bool,
        ui_pass: &mut RadialUiPass,
    ) {
        let jump = ui_pass.child_jump("preview-controls");
        ui.horizontal_wrapped(|ui| {
            compact_section_heading(ui, "Preview", None);
            let combo_identity = format!(
                "combo/{:016x}",
                ui.make_persistent_id("radial-preview-preset").value()
            );
            let combo_jump = jump.as_ref().map(|jump| jump.combo_popup(&combo_identity));
            let preset_combo = egui::ComboBox::from_id_salt("radial-preview-preset")
                .selected_text(format!("{:?}", self.preview_preset))
                .show_ui(ui, |ui| {
                    for (index, preset) in [
                        PreviewPreset::Current,
                        PreviewPreset::OneRing,
                        PreviewPreset::MultiRing,
                        PreviewPreset::Submenu,
                        PreviewPreset::LongLabels,
                        PreviewPreset::HighDpi,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let option = ui
                            .scope_builder(
                                egui::UiBuilder::new()
                                    .id(ui.id().with(("radial-preview-preset-option", index))),
                                |ui| {
                                    ui.selectable_value(
                                        &mut self.preview_preset,
                                        preset,
                                        format!("{preset:?}"),
                                    )
                                },
                            )
                            .inner;
                        register_radial_combo_option(
                            combo_jump.as_ref(),
                            ui,
                            &option,
                            &format!("{combo_identity}/option/{index}"),
                            "Preview preset",
                        );
                    }
                });
            ui_pass.record_combo(&preset_combo.response);
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &preset_combo.response,
                    &combo_identity,
                    "Preview preset selector",
                    TargetKind::ComboBox,
                );
            }
            let zoom_response = ui.add(
                egui::Slider::new(&mut self.preview_zoom, 0.5..=2.0)
                    .text("Zoom")
                    .logarithmic(true),
            );
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &zoom_response,
                    "zoom",
                    "Preview zoom",
                    TargetKind::Slider,
                );
            }
            let zoom_changed = zoom_response.changed();
            trace_pointer_release_response(
                ui,
                &zoom_response,
                WidgetCategory::Zoom,
                zoom_changed,
                trace_correlation(self.session.as_ref()),
            );
            if zoom_changed {
                self.mark_preferences_changed();
            }
            let resources = ui.selectable_label(self.show_resources, "Skins, assets & packages");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &resources,
                    "resources",
                    "Skins and assets",
                    TargetKind::Button,
                );
            }
            if resources.clicked() {
                self.show_resources = !self.show_resources;
                self.mark_preferences_changed();
            }
            secondary_help(ui, "Preview controls are UI-only");
            ui.separator();
            let sample_context =
                ui.checkbox(&mut self.sample_native_context, "Sample external context");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &sample_context,
                    "sample-context",
                    "Sample external context",
                    TargetKind::Button,
                );
            }
            let native_active = self
                .session
                .as_ref()
                .and_then(|session| session.native_preview_lease.as_ref())
                .is_some();
            let correlation = trace_correlation(self.session.as_ref());
            if !native_active {
                let response = ui.button("Open desktop preview");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &response,
                        "open-desktop-preview",
                        "Open desktop preview",
                        TargetKind::Button,
                    );
                }
                trace_designer_authoring_control(
                    ui,
                    &response,
                    DesignerAuthoringTarget::OpenDesktopPreview,
                    DesignerAuthoringRole::Button,
                    None,
                    true,
                    false,
                    ViewportClass::Deferred,
                    correlation,
                );
                if response.clicked() {
                    self.send_native_preview(false);
                }
            }
            if native_active {
                let update = ui.button("Update desktop preview");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &update,
                        "update-desktop-preview",
                        "Update desktop preview",
                        TargetKind::Button,
                    );
                }
                if update.clicked() {
                    self.send_native_preview(true);
                }
            }
            if native_active {
                let response = ui.button("Stop desktop preview");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &response,
                        "stop-desktop-preview",
                        "Stop desktop preview",
                        TargetKind::Button,
                    );
                }
                trace_designer_authoring_control(
                    ui,
                    &response,
                    DesignerAuthoringTarget::StopDesktopPreview,
                    DesignerAuthoringRole::Button,
                    None,
                    true,
                    false,
                    ViewportClass::Deferred,
                    correlation,
                );
                if response.clicked() {
                    self.stop_native_preview();
                }
            }
            if let Some(session) = &self.session {
                ui.small(if session.pending_native_preview.is_some() {
                    "Desktop preview request pending"
                } else if native_active {
                    "Desktop preview active (actions disabled)"
                } else {
                    "Desktop preview stopped"
                });
                if let Some(context) = session.sampled_preview_context.as_ref() {
                    let context_label = context
                        .preferred_external()
                        .map(|window| format!("Context: {}", window.title))
                        .unwrap_or_else(|| "Context: synthetic".into());
                    ui.small(context_label);
                }
                show_radial_diagnostics(
                    ui,
                    session.native_preview_diagnostics.iter(),
                    show_expected_layout_diagnostics,
                    "native-preview",
                );
            }
        });
    }

    fn resources_ui(
        &mut self,
        ui: &mut egui::Ui,
        viewport: ViewportClass,
        ui_pass: &mut RadialUiPass,
    ) {
        if let Some(session) = self.session.as_mut() {
            if self.appearance.destination.is_none() {
                self.appearance.destination = self.visited_path.current().cloned();
            }
            let prepared = self.appearance.prepared_identity(session, &self.preview);
            self.appearance.ui_with_jump_mode(
                ui,
                session,
                self.client.as_ref(),
                viewport,
                prepared,
                ui_pass,
            );
            ui.separator();
            compact_section_heading(ui, "Advanced style, assets and packages", None);
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let mut navigation = None;
        compact_section_heading(ui, "Skins, assets, import and export", None);
        secondary_help(
            ui,
            "Application defaults (read-only) → user defaults → skin → menu → ring → cell",
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("Skin:");
            if ui.button("User defaults").clicked() {
                navigation = Some(DesignerNavigationIntent::ClearSelection);
            }
            for (skin_index, skin) in session.draft.skins.clone().into_iter().enumerate() {
                // Name the authored editing action separately from the
                // temporary skin-preview tiles above it.
                let response = ui.button(format!("Edit {}", skin.name));
                trace_designer_authoring_control(
                    ui,
                    &response,
                    DesignerAuthoringTarget::SkinRow,
                    DesignerAuthoringRole::Button,
                    Some(skin_index),
                    true,
                    session.selection == Some(StableSelection::Skin(skin.id.clone())),
                    viewport,
                    trace_correlation(Some(session)),
                );
                if response.clicked() {
                    navigation = Some(DesignerNavigationIntent::Select {
                        target: StableSelection::Skin(skin.id),
                        control: false,
                        shift: false,
                        force_history: false,
                        open_properties: false,
                    });
                }
            }
            if ui.button("New skin").clicked() {
                navigation = Some(DesignerNavigationIntent::CreateSkin);
            }
        });
        if let Some(StableSelection::Skin(skin_id)) = session.selection.clone()
            && let Some(skin) = session.draft.skins.iter().find(|skin| skin.id == skin_id)
        {
            let mut name = skin.name.clone();
            let response = ui.text_edit_singleline(&mut name);
            if response.changed() || response.lost_focus() {
                let _ =
                    skin_editor::rename_skin(session, &skin_id, name, widget_edit_phase(&response));
            }
            if ui.button("Duplicate skin").clicked() {
                navigation = Some(DesignerNavigationIntent::DuplicateSkin(skin_id.clone()));
            }
            let impact = crate::radial::store::references_to_skin(&session.draft, &skin_id);
            if impact.paths.is_empty() {
                let delete = ui.button("Delete skin");
                if with_button_emphasis(ui, delete, ButtonEmphasis::Destructive).clicked() {
                    let _ = skin_editor::delete_skin(session, &skin_id);
                }
            } else {
                ui.label(format!("In use: {}", impact.paths.join(", ")));
            }
        }
        let scope = match session.selection.clone() {
            Some(StableSelection::Skin(id)) => Some(skin_editor::StyleScope::Skin(id)),
            Some(StableSelection::Menu(id)) => Some(skin_editor::StyleScope::Menu(id)),
            Some(StableSelection::Ring { menu_id, ring_id }) => {
                Some(skin_editor::StyleScope::Ring { menu_id, ring_id })
            }
            Some(StableSelection::Cell {
                menu_id,
                ring_id,
                cell_id,
            }) => Some(skin_editor::StyleScope::Cell {
                menu_id,
                ring_id,
                cell_id,
            }),
            _ => Some(skin_editor::StyleScope::UserDefaults),
        };
        if let Some(scope) = scope {
            labeled_form_row(ui, "Style scope", |ui| ui.label(format!("{scope:?}")));
            if ui.button("Reset this scope to inherited values").clicked() {
                let _ = skin_editor::reset_scope(session, &scope);
            }
            match skin_editor::rows(&session.draft, &scope) {
                Ok(rows) => {
                    egui::ScrollArea::vertical()
                        .id_salt("radial-style-fields")
                        .max_height(240.0)
                        .show(ui, |ui| {
                            for (style_index, row) in rows.into_iter().enumerate() {
                                let input_key = format!("{scope:?}.{}.{}", row.section, row.field);
                                let kind = skin_editor::control_kind(&row.section, &row.field)
                                    .expect("style schema must have an editor control");
                                let text_input = self
                                    .style_text_inputs
                                    .entry(input_key.clone())
                                    .or_default();
                                ui.push_id(
                                    menu::widget_key(
                                        "style",
                                        &format!("{scope:?}"),
                                        &format!("{}.{}", row.section, row.field),
                                    ),
                                    |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(format!("{}.{}", row.section, row.field));
                                            if ui.button("Inherit").clicked() {
                                                let _ = skin_editor::set_override(
                                                    session,
                                                    &scope,
                                                    &row.section,
                                                    &row.field,
                                                    skin_editor::inherit_value(),
                                                );
                                            }
                                            if ui.button("Set").clicked() {
                                                let _ = skin_editor::set_override(
                                                    session,
                                                    &scope,
                                                    &row.section,
                                                    &row.field,
                                                    row.inherited.clone(),
                                                );
                                            }
                                            if ui.button("Clear").clicked() {
                                                let _ = skin_editor::set_override(
                                                    session,
                                                    &scope,
                                                    &row.section,
                                                    &row.field,
                                                    skin_editor::clear_value(),
                                                );
                                            }
                                            if let Some((value, phase)) = style_value_widget(
                                                ui,
                                                kind,
                                                &row.current,
                                                &row.inherited,
                                                text_input,
                                                &session.font_families,
                                                &format!("{}.{}", row.section, row.field),
                                                (row.section == "effects"
                                                    && row.field == "glow_enabled")
                                                    .then_some((
                                                        DesignerAuthoringTarget::SkinGlowEnabled,
                                                        viewport,
                                                        Some(style_index),
                                                        trace_correlation(Some(session)),
                                                    )),
                                                Some(ui_pass),
                                            ) && let Err(error) = skin_editor::set_override_edit(
                                                session,
                                                &scope,
                                                &row.section,
                                                &row.field,
                                                value,
                                                phase,
                                            ) {
                                                self.resource_notice = Some(ResourceNotice::error(error));
                                            }
                                            if skin_editor::is_media_field(&row.section, &row.field) {
                                                let media_kind = if row.section == "sounds" {
                                                    crate::radial::model::MediaKind::Sound
                                                } else {
                                                    crate::radial::model::MediaKind::Image
                                                };
                                                for (label, managed) in [
                                                    ("Managed file", true),
                                                    ("External file", false),
                                                ] {
                                                    if ui.button(label).clicked() {
                                                        self.intent_bridge.enqueue_file_dialog(
                                                            DesignerFileDialogRequest::StyleMedia {
                                                                scope: scope.clone(),
                                                                section: row.section.clone(),
                                                                field: row.field.clone(),
                                                                media_kind,
                                                                managed,
                                                            },
                                                        );
                                                    }
                                                }
                                                if ui.button("Search-path").clicked()
                                                    && !text_input.trim().is_empty()
                                                {
                                                    let value = crate::radial::model::MediaReference::SearchPath {
                                                        file_name: text_input.trim().to_owned(),
                                                    };
                                                    let _ = skin_editor::set_override(
                                                        session,
                                                        &scope,
                                                        &row.section,
                                                        &row.field,
                                                        skin_editor::with_payload(
                                                            serde_json::to_value(value).unwrap_or_default(),
                                                        ),
                                                    );
                                                }
                                                if kind == skin_editor::StyleControlKind::Resource
                                                    && row.section == "images"
                                                    && ui.button("Icon resource").clicked()
                                                    && !text_input.trim().is_empty()
                                                {
                                                    let value = crate::radial::model::MediaReference::IconResource {
                                                        path: text_input.trim().to_owned(),
                                                        index: 1,
                                                    };
                                                    let _ = skin_editor::set_override(
                                                        session,
                                                        &scope,
                                                        &row.section,
                                                        &row.field,
                                                        skin_editor::with_payload(
                                                            serde_json::to_value(value).unwrap_or_default(),
                                                        ),
                                                    );
                                                }
                                            }
                                            ui.small(format!("inherited from {:?}", row.source));
                                        });
                                    },
                                );
                            }
                        });
                }
                Err(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, format!("Error: {error}"));
                }
            }
        }
        document_rule_controls(
            ui,
            session,
            &mut self.preferences.expanded_sections,
            ui_pass,
        );
        ui.separator();
        compact_section_heading(ui, "Managed assets", None);
        ui.horizontal_wrapped(|ui| {
            for (label, kind, extensions) in [
                (
                    "Import managed image",
                    crate::radial::model::MediaKind::Image,
                    &["png", "gif", "jpg", "jpeg"][..],
                ),
                (
                    "Import managed WAV",
                    crate::radial::model::MediaKind::Sound,
                    &["wav"][..],
                ),
            ] {
                if ui.button(label).clicked() {
                    self.intent_bridge.enqueue_file_dialog(
                        DesignerFileDialogRequest::ImportManaged {
                            kind,
                            label: label.to_owned(),
                            extensions: extensions
                                .iter()
                                .map(|extension| (*extension).to_owned())
                                .collect(),
                        },
                    );
                }
            }
        });
        for asset in session.draft.assets.clone() {
            ui.horizontal(|ui| {
                ui.label(format!("{} ({:?})", asset.id, asset.kind));
                if asset.kind == crate::radial::model::MediaKind::Sound
                    && ui.button("Audition").clicked()
                {
                    if let Some(addition) = session
                        .pending_assets
                        .additions
                        .iter()
                        .find(|addition| addition.record.id == asset.id)
                    {
                        let _ = audio_controls::audition(std::sync::Arc::clone(&addition.bytes));
                    } else {
                        match session.request_audition_managed_asset(asset.id.clone()) {
                            Ok(request) => {
                                let pending = session.pending_request;
                                let result = self.client.as_ref().map_or_else(
                                    || Err(AuthoringError::ServiceClosed),
                                    |client| client.send(request),
                                );
                                if let Err(error) = result
                                    && let Some(pending) = pending
                                {
                                    session.reconcile_request_delivery_failure(
                                        pending,
                                        format!("{error:?}"),
                                    );
                                }
                            }
                            Err(error) => session.last_error = Some(format!("{error:?}")),
                        }
                    }
                }
                let impact = asset_picker::delete_impact(session, &asset.id);
                if impact.paths.is_empty() {
                    let delete = ui.button("Delete");
                    if with_button_emphasis(ui, delete, ButtonEmphasis::Destructive).clicked() {
                        let _ = asset_picker::delete_managed_asset(session, &asset.id);
                    }
                } else {
                    ui.label(format!("In use: {}", impact.paths.join(", ")));
                }
            });
        }
        if let Some(path) = session.last_backup_path.as_ref() {
            ui.label(format!("Verified replacement backup: {}", path.display()));
        }
        if let Some(intent) = navigation {
            queue_scope_change(
                &mut self.pending_scope_change,
                session,
                ScopeChangeIntent::Navigation(intent),
            );
        }
        self.package_controls(ui);
        if let Some(notice) = &self.resource_notice {
            show_resource_notice(ui, notice);
        }
    }

    fn package_controls(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button("Preview .mlradial import").clicked() {
                self.intent_bridge
                    .enqueue_file_dialog(DesignerFileDialogRequest::PreviewPackage);
            }
            if ui.button("Export selected menu").clicked()
                && let Some(menu_id) = selected_menu_id(session)
            {
                self.intent_bridge
                    .enqueue_file_dialog(DesignerFileDialogRequest::ExportMenu { menu_id });
            }
            if ui.button("Export full package").clicked() {
                let roots = session
                    .draft
                    .menus
                    .iter()
                    .map(|menu| menu.id.clone())
                    .collect::<Vec<_>>();
                self.intent_bridge
                    .enqueue_file_dialog(DesignerFileDialogRequest::ExportPackage { roots });
            }
            if ui.button("Export selected skin").clicked()
                && let Some(StableSelection::Skin(skin_id)) = session.selection.clone()
            {
                self.intent_bridge
                    .enqueue_file_dialog(DesignerFileDialogRequest::ExportSkin { skin_id });
            }
            for (label, source) in [
                (
                    "Preview Radify files",
                    crate::radial::compatibility::CompatibilitySource::Radify,
                ),
                (
                    "Preview RM4 files",
                    crate::radial::compatibility::CompatibilitySource::RadialMenuV4,
                ),
            ] {
                if ui.button(label).clicked() {
                    self.intent_bridge
                        .enqueue_file_dialog(DesignerFileDialogRequest::PreviewLegacy { source });
                }
            }
        });
        if let Some(preview) = self.pending_import.clone() {
            ui.group(|ui| {
                ui.label("Import preview (no files or draft changed)");
                ui.label(format!("Destination: {}", preview.destination()));
                for mapping in preview.mappings() {
                    ui.label(mapping);
                }
                for warning in preview.warnings() {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("Warning: {warning}"),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button("Create new (default)").clicked() {
                        self.pending_import = None;
                        self.replace_confirmed = false;
                        self.replace_backup_path = None;
                        if let Err(error) = preview.clone().accept_create_new(session) {
                            self.resource_notice = Some(ResourceNotice::error(format!("{error:?}")));
                        }
                    }
                    if ui.button("Cancel preview").clicked() {
                        self.pending_import = None;
                        self.replace_confirmed = false;
                        self.replace_backup_path = None;
                    }
                });
                let replace_plan = preview.plan();
                ui.separator();
                ui.add_enabled_ui(replace_plan.is_some(), |ui| {
                ui.label("Replace the entire radial document");
                ui.checkbox(
                    &mut self.replace_confirmed,
                    "I understand this replaces all current radial menus, skins, and assets",
                );
                ui.horizontal(|ui| {
                    if ui.button("Choose verified backup destination…").clicked() {
                        self.intent_bridge.enqueue_file_dialog(
                            DesignerFileDialogRequest::ReplaceBackup,
                        );
                    }
                    if let Some(path) = &self.replace_backup_path {
                        ui.label(path.display().to_string());
                    }
                });
                let ready = self.replace_confirmed && self.replace_backup_path.is_some();
                let replace = ui.add_enabled(
                    ready,
                    egui::Button::new("Replace using verified backup"),
                );
                if with_button_emphasis(ui, replace, ButtonEmphasis::Destructive).clicked() {
                    let backup_path = self.replace_backup_path.clone().unwrap_or_default();
                    match session.request_replace_package(
                        replace_plan.clone().expect("replace UI requires a menu package"),
                        preview.revision,
                        &preview.disk_sha256,
                        preview.generation,
                        backup_path,
                        self.replace_confirmed,
                    ) {
                        Ok(request) => {
                            let pending = session.pending_request;
                            let result = self.client.as_ref().map_or_else(
                                || Err(AuthoringError::ServiceClosed),
                                |client| client.send(request),
                            );
                            match result {
                                Ok(()) => {
                                    self.pending_import = None;
                                    self.replace_confirmed = false;
                                    self.replace_backup_path = None;
                                }
                                Err(error) => {
                                    if let Some(pending) = pending {
                                        session.reconcile_request_delivery_failure(
                                            pending,
                                            format!("{error:?}"),
                                        );
                                    }
                                }
                            }
                        }
                        Err(error) => session.last_error = Some(format!("{error:?}")),
                    }
                }
                });
                if replace_plan.is_none() {
                    ui.small("Skin bundles merge as one undoable draft edit and cannot replace the full document.");
                }
            });
        }
    }

    fn basic_authoring_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        defaults: &crate::radial::model::RadialFeatureSettings,
        viewport: ViewportClass,
        ui_pass: &mut RadialUiPass,
    ) {
        let jump = ui_pass.child_jump("basic-authoring");
        let mut create_menu = false;
        let mut propose_new_ring = false;
        let mut preview_resize = false;
        let mut apply_proposal = false;
        let mut cancel_proposal = false;
        let mut choose_ring = None;

        let selected_menu = self
            .session
            .as_ref()
            .and_then(selected_menu_id)
            .or_else(|| {
                self.session
                    .as_ref()
                    .map(|session| session.draft.default_menu_id.clone())
            });
        let selected_ring = self
            .session
            .as_ref()
            .and_then(|session| match &session.selection {
                Some(StableSelection::Ring { menu_id, ring_id })
                | Some(StableSelection::Cell {
                    menu_id, ring_id, ..
                }) => Some((menu_id.clone(), ring_id.clone())),
                Some(StableSelection::CellSet(cells)) => {
                    Some((cells.primary.menu_id.clone(), cells.primary.ring_id.clone()))
                }
                _ => selected_menu.as_ref().and_then(|menu_id| {
                    session
                        .draft
                        .menus
                        .iter()
                        .find(|menu| &menu.id == menu_id)
                        .and_then(|menu| menu.rings.first())
                        .map(|ring| (menu_id.clone(), ring.id.clone()))
                }),
            });
        let ring_options = selected_ring.as_ref().and_then(|(menu_id, ring_id)| {
            self.session.as_ref().and_then(|session| {
                session
                    .draft
                    .menus
                    .iter()
                    .find(|menu| &menu.id == menu_id)
                    .map(|menu| {
                        let rings = menu
                            .rings
                            .iter()
                            .map(|ring| (ring.id.clone(), ring.cells.len()))
                            .collect::<Vec<_>>();
                        let selected = rings.iter().position(|(id, _)| id == ring_id).unwrap_or(0);
                        (rings, selected)
                    })
            })
        });
        let correlation = trace_correlation(self.session.as_ref());

        ui.group(|ui| {
            compact_section_heading(ui, "Design actions", None);
            ui.horizontal_wrapped(|ui| {
                let response = ui.button("New Menu");
                if let Some(jump) = jump.as_ref() {
                    jump.register(ui, &response, "new-menu", "Create menu", TargetKind::Button);
                }
                trace_designer_authoring_control(
                    ui,
                    &response,
                    DesignerAuthoringTarget::NewMenu,
                    DesignerAuthoringRole::Button,
                    None,
                    true,
                    false,
                    viewport,
                    correlation,
                );
                create_menu = response.clicked();

                let response = ui
                    .add_enabled(selected_menu.is_some(), egui::Button::new("Add Ring"))
                    .on_disabled_hover_text("Select a menu first")
                    ;
                if let Some(jump) = jump.as_ref() {
                    jump.register(ui, &response, "add-ring", "Add ring", TargetKind::Button);
                }
                trace_designer_authoring_control(
                    ui,
                    &response,
                    DesignerAuthoringTarget::AddRing,
                    DesignerAuthoringRole::Button,
                    None,
                    response.enabled(),
                    false,
                    viewport,
                    correlation,
                );
                propose_new_ring = response.clicked();

                if let (Some((menu_id, ring_id)), Some((rings, current_index))) =
                    (selected_ring.as_ref(), ring_options.as_ref())
                {
                    let combo_salt = "Ring";
                    let combo_identity = format!(
                        "combo/{:016x}",
                        ui.make_persistent_id(combo_salt).value()
                    );
                    let ring_popup = jump
                        .as_ref()
                        .map(|jump| jump.combo_popup(&combo_identity));
                    let combo = egui::ComboBox::from_id_salt(combo_salt)
                        .selected_text(format!(
                            "{} · {} slots",
                            current_index + 1,
                            rings.get(*current_index).map_or(0, |(_, count)| *count)
                        ))
                        .show_ui(ui, |ui| {
                            for (index, (candidate_id, count)) in rings.iter().enumerate() {
                                let response = ui.selectable_label(
                                    candidate_id == ring_id,
                                    format!("Ring {} · {} slots", index + 1, count),
                                );
                                let ring_selection = StableSelection::Ring {
                                    menu_id: menu_id.clone(),
                                    ring_id: candidate_id.clone(),
                                };
                                let ring_identity = radial_jump_selection_identity(&ring_selection);
                                register_radial_combo_option(
                                    ring_popup.as_ref(),
                                    ui,
                                    &response,
                                    &format!("ring-option/{ring_identity}"),
                                    "Ring option",
                                );
                                trace_designer_authoring_control(
                                    ui,
                                    &response,
                                    DesignerAuthoringTarget::RingOption,
                                    DesignerAuthoringRole::Selectable,
                                    Some(index),
                                    true,
                                    candidate_id == ring_id,
                                    viewport,
                                    correlation,
                                );
                                if response.clicked() {
                                    choose_ring = Some(ring_selection);
                                }
                            }
                        });
                    register_radial_combo_anchor(
                        ui_pass,
                        &combo.response,
                        &combo_identity,
                        "Ring selector",
                    );
                    if let Some(jump) = jump.as_ref() {
                        jump.register(
                            ui,
                            &combo.response,
                            &combo_identity,
                            "Ring selector",
                            TargetKind::ComboBox,
                        );
                    }
                    trace_designer_authoring_control(
                        ui,
                        &combo.response,
                        DesignerAuthoringTarget::RingSelector,
                        DesignerAuthoringRole::ComboBox,
                        None,
                        true,
                        true,
                        viewport,
                        correlation,
                    );
                    if let Some((current_ring, count)) = rings.get(*current_index) {
                        let key = (menu_id.clone(), current_ring.clone());
                        let requested = self.ring_resize_drafts.entry(key).or_insert(*count);
                        let response = ui.add(
                            egui::DragValue::new(requested)
                                .prefix("Slots ")
                                .range(0..=crate::radial::model::limits::MAX_CELLS_PER_RING),
                        );
                        if let Some(jump) = jump.as_ref() {
                            jump.register(ui, &response, "ring-slots", "Ring slot count", TargetKind::Slider);
                        }
                        trace_designer_authoring_control(
                            ui,
                            &response,
                            DesignerAuthoringTarget::Slots,
                            DesignerAuthoringRole::DragValue,
                            None,
                            true,
                            false,
                            viewport,
                            correlation,
                        );
                        let response = ui.button("Preview proposal");
                        if let Some(jump) = jump.as_ref() {
                            jump.register(ui, &response, "preview-proposal", "Preview ring proposal", TargetKind::Button);
                        }
                        trace_designer_authoring_control(
                            ui,
                            &response,
                            DesignerAuthoringTarget::PreviewProposal,
                            DesignerAuthoringRole::Button,
                            None,
                            true,
                            false,
                            viewport,
                            correlation,
                        );
                        preview_resize = response.clicked();
                    }
                } else {
                    ui.label("Ring: select a menu");
                }
            });
            if let Some(proposal) = &self.ring_proposal {
                let proposal_token = self
                    .ring_proposal_nonce
                    .map(|nonce| ring_proposal_token(proposal, nonce))
                    .unwrap_or_default();
                let proposal_ready = !proposal_token.is_empty()
                    && self.session.as_ref().is_some_and(|session| {
                        self.preview
                            .candidate_is_prepared(session, &proposal_token)
                    });
                let proposal_failure = (!proposal_token.is_empty())
                    .then(|| self.preview.candidate_failure(&proposal_token))
                    .flatten()
                    .map(str::to_owned);
                ui.horizontal_wrapped(|ui| {
                    let change = proposal.previous_radius.map_or_else(
                        || format!("new outer ring at radius {:.1}", proposal.proposed_radius),
                        |previous| {
                            format!(
                                "{} slots; radius {:.1} → {:.1}",
                                proposal.requested_len, previous, proposal.proposed_radius
                            )
                        },
                    );
                    ui.label(format!("Proposal: {change}"));
                    if let Some(summary) = &proposal.resolution_summary {
                        ui.colored_label(ui.visuals().warn_fg_color, summary);
                    }
                    if let Some(failure) = proposal_failure.as_deref() {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!("Preview failed: {failure}"),
                        );
                    } else if !proposal_ready {
                        ui.spinner();
                        ui.small("Preparing candidate preview…");
                    }
                    let button = ui
                        .add_enabled(proposal_ready, egui::Button::new("Apply proposal"))
                        .on_disabled_hover_text(
                            "Apply is available only after this exact candidate is prepared and visible",
                        );
                    let response = with_button_emphasis(ui, button, ButtonEmphasis::Primary);
                    if let Some(jump) = jump.as_ref() {
                        jump.register(
                            ui,
                            &response,
                            "apply-proposal",
                            "Apply ring proposal",
                            TargetKind::Button,
                        );
                    }
                    trace_designer_authoring_control(
                        ui,
                        &response,
                        DesignerAuthoringTarget::ApplyProposal,
                        DesignerAuthoringRole::Button,
                        None,
                        proposal_ready,
                        false,
                        viewport,
                        correlation,
                    );
                    apply_proposal = response.clicked();
                    let response = ui.button("Cancel proposal");
                    if let Some(jump) = jump.as_ref() {
                        jump.register(
                            ui,
                            &response,
                            "cancel-proposal",
                            "Cancel ring proposal",
                            TargetKind::Button,
                        );
                    }
                    trace_designer_authoring_control(
                        ui,
                        &response,
                        DesignerAuthoringTarget::CancelProposal,
                        DesignerAuthoringRole::Button,
                        None,
                        true,
                        false,
                        viewport,
                        correlation,
                    );
                    cancel_proposal = response.clicked();
                });
            }
        });

        if let Some(selection) = choose_ring {
            self.queue_navigation_intent(DesignerNavigationIntent::Select {
                target: selection,
                control: false,
                shift: false,
                force_history: false,
                open_properties: false,
            });
        }

        if create_menu {
            self.queue_editor_operation(DeferredEditorOperation::CreateMenu {
                interaction: defaults.default_interaction,
                submenu_presentation: defaults.default_submenu_presentation,
            });
        }
        if propose_new_ring
            && let (Some(session), Some(menu_id)) = (self.session.as_mut(), selected_menu.as_ref())
        {
            let ring_id = session.allocate_ring_id("ring");
            let cell_ids = (0..8)
                .map(|index| session.allocate_cell_id(&format!("cell-{index}")))
                .collect();
            match menu::propose_new_ring(
                &session.draft,
                session.generation,
                menu_id,
                ring_id,
                cell_ids,
            ) {
                Ok(proposal) => self.install_ring_proposal(proposal),
                Err(error) => session.last_error = Some(format!("Add ring failed: {error:?}")),
            }
        }
        if preview_resize
            && let (Some(session), Some((menu_id, ring_id))) =
                (self.session.as_mut(), selected_ring.as_ref())
        {
            let count = session
                .draft
                .menus
                .iter()
                .find(|menu| &menu.id == menu_id)
                .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
                .map_or(0, |ring| ring.cells.len());
            let requested = self
                .ring_resize_drafts
                .get(&(menu_id.clone(), ring_id.clone()))
                .copied()
                .unwrap_or(count);
            match menu::resize_plan(&session.draft, menu_id, ring_id, requested) {
                Ok(plan) if plan.requires_resolution() => {
                    self.resize_prompt = Some(plan);
                    self.ring_proposal = None;
                    self.ring_proposal_nonce = None;
                }
                Ok(_) => {
                    let ids = (0..requested.saturating_sub(count))
                        .map(|_| session.allocate_cell_id("cell"))
                        .collect();
                    match menu::propose_ring_resize(
                        &session.draft,
                        session.generation,
                        menu_id,
                        ring_id,
                        requested,
                        ids,
                    ) {
                        Ok(proposal) => self.install_ring_proposal(proposal),
                        Err(error) => {
                            session.last_error = Some(format!("Slot proposal failed: {error:?}"))
                        }
                    }
                }
                Err(error) => session.last_error = Some(format!("Slot proposal failed: {error:?}")),
            }
        }
        if apply_proposal
            && let (Some(proposal), Some(nonce)) =
                (self.ring_proposal.clone(), self.ring_proposal_nonce)
        {
            self.queue_editor_operation(DeferredEditorOperation::ApplyRingProposal {
                proposal: Box::new(proposal),
                nonce,
            });
        }
        if cancel_proposal {
            self.ring_proposal = None;
            self.ring_proposal_nonce = None;
        }
        self.trace_authoring_geometry(None);
    }

    fn install_ring_proposal(&mut self, proposal: menu::RingEditProposal) {
        let nonce = self.next_ring_proposal_nonce;
        self.next_ring_proposal_nonce = self.next_ring_proposal_nonce.wrapping_add(1).max(1);
        self.ring_proposal = Some(proposal);
        self.ring_proposal_nonce = Some(nonce);
        self.preview.retry_preparation();
    }

    fn trace_authoring_geometry(
        &self,
        action_catalog: Option<
            &crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot,
        >,
    ) {
        if !acceptance_trace::enabled() {
            return;
        }
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let selected_menu_id = session
            .selection
            .as_ref()
            .and_then(|selection| match selection {
                StableSelection::Menu(menu_id)
                | StableSelection::Ring { menu_id, .. }
                | StableSelection::Cell { menu_id, .. } => Some(menu_id),
                StableSelection::CellSet(cells) => Some(&cells.primary.menu_id),
                _ => None,
            });
        let selected_menu_index = selected_menu_id.and_then(|menu_id| {
            session
                .draft
                .menus
                .iter()
                .position(|menu| &menu.id == menu_id)
        });
        let selected_menu = selected_menu_id
            .and_then(|menu_id| session.draft.menus.iter().find(|menu| &menu.id == menu_id));
        let selected_ring_id = session
            .selection
            .as_ref()
            .and_then(|selection| match selection {
                StableSelection::Ring { ring_id, .. } | StableSelection::Cell { ring_id, .. } => {
                    Some(ring_id)
                }
                StableSelection::CellSet(cells) => Some(&cells.primary.ring_id),
                _ => None,
            });
        let selected_ring_index = selected_menu.and_then(|menu| {
            selected_ring_id
                .and_then(|ring_id| menu.rings.iter().position(|ring| &ring.id == ring_id))
                .or_else(|| (!menu.rings.is_empty()).then_some(0))
        });
        let selected_ring = selected_menu
            .and_then(|menu| selected_ring_index.and_then(|index| menu.rings.get(index)));
        let selected_cell = session
            .selection
            .as_ref()
            .and_then(|selection| match selection {
                StableSelection::Cell {
                    menu_id,
                    ring_id,
                    cell_id,
                } => session
                    .draft
                    .menus
                    .iter()
                    .find(|menu| &menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
                    .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == cell_id)),
                StableSelection::CellSet(cells) => session
                    .draft
                    .menus
                    .iter()
                    .find(|menu| menu.id == cells.primary.menu_id)
                    .and_then(|menu| {
                        menu.rings
                            .iter()
                            .find(|ring| ring.id == cells.primary.ring_id)
                    })
                    .and_then(|ring| {
                        ring.cells
                            .iter()
                            .find(|cell| cell.id == cells.primary.cell_id)
                    }),
                _ => None,
            });
        let selected_cell_index = selected_cell.and_then(|cell| {
            selected_ring.and_then(|ring| {
                ring.cells
                    .iter()
                    .position(|candidate| candidate.id == cell.id)
            })
        });
        let selected_cell_id_digest =
            selected_cell.map(|cell| stable_cell_id_digest(cell.id.as_str()));
        let selected_cell_action_reference = selected_cell.and_then(|cell| match &cell.content {
            CellContent::Action {
                binding: crate::radial::model::ActionBinding::Persisted { action },
            } => Some(action.clone()),
            _ => None,
        });
        let (selected_cell_custom_action_index, selected_cell_custom_action_index_known) =
            selected_cell_action_reference.map_or((None, true), |reference| {
                let Some(action_catalog) = action_catalog else {
                    return (None, false);
                };
                let catalog = action_catalog.persisted_catalog();
                let context = crate::universal_actions::ActionResolutionContext::new(
                    crate::universal_actions::ActionSurface::RadialMenu,
                    "",
                );
                let index = catalog
                    .resolve(
                        &reference,
                        &crate::universal_actions::UniversalActionRegistry,
                        &context,
                    )
                    .ok()
                    .and_then(|resolved| resolved.target.custom_action_index);
                (index, true)
            });
        let requested_slots = selected_menu.zip(selected_ring).map_or(0, |(menu, ring)| {
            self.ring_resize_drafts
                .get(&(menu.id.clone(), ring.id.clone()))
                .copied()
                .unwrap_or(ring.cells.len())
        });
        let menu_populated = selected_menu.map_or(0, |menu| {
            menu.rings
                .iter()
                .flat_map(|ring| &ring.cells)
                .filter(|cell| !matches!(&cell.content, CellContent::Spacer))
                .count()
        });
        let proposal = self.ring_proposal.as_ref();
        let proposal_token = self
            .ring_proposal
            .as_ref()
            .zip(self.ring_proposal_nonce)
            .map(|(proposal, nonce)| ring_proposal_token(proposal, nonce));
        let proposal_ready = proposal_token
            .as_deref()
            .is_some_and(|token| self.preview.candidate_is_prepared(session, token));
        let proposal_kind = proposal.map_or(DesignerProposalKind::None, |proposal| {
            if proposal.resolution_summary.is_some() {
                DesignerProposalKind::ResolvedResize
            } else if proposal.previous_radius.is_some() {
                DesignerProposalKind::Resize
            } else {
                DesignerProposalKind::NewRing
            }
        });
        let draft_cell_ids_digest = selected_menu.map_or(0, menu_cell_ids_digest);
        let proposal_cell_ids_digest = proposal.and_then(|proposal| {
            proposal
                .document
                .menus
                .iter()
                .find(|menu| menu.id == proposal.menu_id)
                .map(menu_cell_ids_digest)
        });
        let (proposal_candidate_rings, proposal_cell_ids_preserved, proposal_resolution_populated) =
            proposal.map_or((0, true, 0), |proposal| {
                let original_menu = session
                    .draft
                    .menus
                    .iter()
                    .find(|menu| menu.id == proposal.menu_id);
                let candidate_menu = proposal
                    .document
                    .menus
                    .iter()
                    .find(|menu| menu.id == proposal.menu_id);
                let candidate_rings = candidate_menu.map_or(0, |menu| menu.rings.len());
                let ids_preserved = match (original_menu, candidate_menu) {
                    (Some(original), Some(candidate)) if proposal.resolution_summary.is_some() => {
                        original
                            .rings
                            .iter()
                            .flat_map(|ring| &ring.cells)
                            .all(|cell| {
                                candidate.rings.iter().any(|ring| {
                                    ring.cells.iter().any(|candidate| candidate.id == cell.id)
                                })
                            })
                    }
                    (Some(original), Some(candidate)) if proposal.previous_radius.is_some() => {
                        original
                            .rings
                            .iter()
                            .find(|ring| ring.id == proposal.ring_id)
                            .zip(
                                candidate
                                    .rings
                                    .iter()
                                    .find(|ring| ring.id == proposal.ring_id),
                            )
                            .is_some_and(|(before, after)| {
                                before
                                    .cells
                                    .iter()
                                    .take(proposal.requested_len)
                                    .zip(after.cells.iter())
                                    .all(|(before, after)| before.id == after.id)
                            })
                    }
                    (Some(original), Some(candidate)) => original
                        .rings
                        .iter()
                        .flat_map(|ring| &ring.cells)
                        .all(|cell| {
                            candidate.rings.iter().any(|ring| {
                                ring.cells.iter().any(|candidate| candidate.id == cell.id)
                            })
                        }),
                    _ => false,
                };
                let resolution_populated = proposal
                    .resolution_summary
                    .as_ref()
                    .and_then(|_| {
                        original_menu
                            .and_then(|menu| {
                                menu.rings.iter().find(|ring| ring.id == proposal.ring_id)
                            })
                            .map(|ring| {
                                ring.cells
                                    .iter()
                                    .skip(proposal.requested_len)
                                    .filter(|cell| !matches!(&cell.content, CellContent::Spacer))
                                    .count()
                            })
                    })
                    .unwrap_or(0);
                (candidate_rings, ids_preserved, resolution_populated)
            });
        let resize_prompt_populated = self
            .resize_prompt
            .as_ref()
            .map_or(0, |plan| plan.populated_removed.len());
        acceptance_trace::emit_designer_geometry_state(DesignerGeometryState {
            session_id: session.editor_session.0,
            menu_count: session.draft.menus.len(),
            selected_menu_index,
            selected_menu_after_action: selected_menu.map(|menu| menu.after_action),
            ring_count: selected_menu.map_or(0, |menu| menu.rings.len()),
            selected_ring_index,
            selected_cell_index,
            selected_cell_id_digest,
            selected_cell_custom_action_index,
            selected_cell_custom_action_index_known,
            selected_ring_slots: selected_ring.map_or(0, |ring| ring.cells.len()),
            requested_slots,
            selected_ring_populated: selected_ring.map_or(0, |ring| {
                ring.cells
                    .iter()
                    .filter(|cell| !matches!(&cell.content, CellContent::Spacer))
                    .count()
            }),
            menu_populated,
            draft_cell_ids_digest,
            proposal_cell_ids_digest: proposal_cell_ids_digest.unwrap_or_default(),
            proposal_cell_ids_digest_available: proposal_cell_ids_digest.is_some(),
            proposal_kind,
            proposal_active: proposal.is_some(),
            proposal_ready,
            proposal_slots: proposal.map_or(0, |proposal| proposal.requested_len),
            proposal_candidate_rings,
            proposal_resolution_populated,
            proposal_cell_ids_preserved,
            resize_prompt_open: self.resize_prompt.is_some(),
            resize_prompt_populated,
            generation: session.generation.0,
        });
    }

    fn tree(
        &mut self,
        ui: &mut egui::Ui,
        _defaults: &crate::radial::model::RadialFeatureSettings,
        viewport: ViewportClass,
        ui_pass: &RadialUiPass,
    ) {
        let jump = ui_pass.child_jump("tree");
        compact_section_heading(ui, "Menus and rings", None);
        let pending_scope_change = &mut self.pending_scope_change;
        let filter = &mut self.tree_filter;
        let drag_source = &mut self.drag_source;
        let post_render = &mut self.post_render;
        let focus_restore = &mut self.focus_restore;
        let expanded_sections = &mut self.preferences.expanded_sections;
        let preferences_dirty = &mut self.preferences_dirty;
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let menus = session.draft.menus.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label("Search");
            let response = ui.add(
                egui::TextEdit::singleline(filter)
                    .hint_text("Authored menus, labels, or IDs")
                    .desired_width(f32::INFINITY),
            );
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &response,
                    "search",
                    "Search menu tree",
                    TargetKind::TextEntry,
                );
            }
            let undo_scope = acceptance_trace::enabled()
                .then(|| designer_tree_search_undo_scope(ui.ctx(), response.id, filter))
                .flatten();
            trace_designer_authoring_control_scoped_rect(
                ui,
                response.rect,
                response.clicked(),
                DesignerAuthoringTarget::TreeSearch,
                DesignerAuthoringRole::TextEdit,
                None,
                true,
                false,
                response.has_focus(),
                viewport,
                trace_correlation(Some(session)),
                undo_scope,
            );
            if !filter.is_empty() {
                let clear = ui.small_button("Clear");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &clear,
                        "clear-search",
                        "Clear tree search",
                        TargetKind::Button,
                    );
                }
                trace_designer_authoring_control(
                    ui,
                    &clear,
                    DesignerAuthoringTarget::TreeSearchClear,
                    DesignerAuthoringRole::Button,
                    None,
                    true,
                    false,
                    viewport,
                    trace_correlation(Some(session)),
                );
                if clear.clicked() {
                    filter.clear();
                }
            }
        });
        if !filter.trim().is_empty() {
            let hits = authored_search_hits(&session.draft, filter);
            secondary_help(ui, &format!("{} authored results", hits.len()));
            show_designer_scroll_area(
                ui,
                egui::ScrollArea::vertical().id_salt("radial-designer-search-results"),
                DesignerAuthoringScrollOwner::MenuTree,
                viewport,
                trace_correlation(Some(session)),
                |ui| {
                    for (hit_index, hit) in hits.into_iter().enumerate() {
                        // Reuse the widget's actual Galley so diagnostic text geometry
                        // describes what is painted, including wrapping and elision.
                        let galley =
                            egui::WidgetText::from(format!("{}  ·  {}", hit.label, hit.context))
                                .into_galley(
                                    ui,
                                    None,
                                    ui.available_width() - 2.0 * ui.spacing().button_padding.x,
                                    egui::TextStyle::Button,
                                );
                        let selected = session.selection.as_ref() == Some(&hit.target)
                            || session.selection.as_ref().is_some_and(|selection| {
                                matches!(
                                    (&hit.target, selection),
                                    (
                                        StableSelection::Cell { menu_id: target_menu, ring_id: target_ring, cell_id: target_cell },
                                        StableSelection::CellSet(cells)
                                    ) if target_menu == &cells.primary.menu_id
                                        && cells.members.iter().any(|member| member.menu_id == *target_menu && member.ring_id == *target_ring && member.cell_id == *target_cell)
                                )
                            });
                        let selection_identity = radial_jump_selection_identity(&hit.target);
                        let response = ui
                            .scope_builder(
                                egui::UiBuilder::new().id(radial_native_selection_id(
                                    session,
                                    &hit.target,
                                    "search-result",
                                )),
                                |ui| ui.selectable_label(selected, std::sync::Arc::clone(&galley)),
                            )
                            .inner
                            .on_hover_text(format!("{} · {}", hit.label, hit.context));
                        let selected_by_jump = jump.as_ref().is_some_and(|jump| {
                            jump.register_select(
                                ui,
                                &response,
                                &format!("search-result/{selection_identity}"),
                                "Authored search result",
                            )
                        });
                        let search_scope = match &hit.target {
                            StableSelection::Cell { menu_id, ring_id, cell_id } => designer_rendered_text(ui, &response, &galley).map(|rendered_text|
                                acceptance_trace::DesignerAuthoringControlScope::AuthoredSearchResult {
                                    menu_id_digest: acceptance_identity_digest(&[menu_id.as_str()]),
                                    ring_id_digest: acceptance_identity_digest(&[ring_id.as_str()]),
                                    cell_id_digest: acceptance_identity_digest(&[cell_id.as_str()]),
                                    authored_target_digest: acceptance_identity_digest(&[
                                        menu_id.as_str(), ring_id.as_str(), cell_id.as_str(),
                                    ]),
                                    rendered_text,
                                },
                            ),
                            _ => None,
                        };
                        trace_designer_authoring_control_scoped_rect(
                            ui,
                            response.rect,
                            response.clicked(),
                            DesignerAuthoringTarget::TreeSearchResult,
                            DesignerAuthoringRole::Selectable,
                            Some(hit_index),
                            true,
                            selected,
                            response.has_focus(),
                            viewport,
                            trace_correlation(Some(session)),
                            search_scope,
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                ui.is_enabled(),
                                selected,
                                format!("{} · {}", hit.label, hit.context),
                            )
                        });
                        if response.clicked() || selected_by_jump {
                            queue_scope_change(
                                pending_scope_change,
                                session,
                                ScopeChangeIntent::Navigation(DesignerNavigationIntent::Select {
                                    target: hit.target,
                                    control: false,
                                    shift: false,
                                    force_history: true,
                                    open_properties: false,
                                }),
                            );
                            filter.clear();
                        }
                    }
                },
            );
            return;
        }
        show_designer_scroll_area(
            ui,
            egui::ScrollArea::vertical(),
            DesignerAuthoringScrollOwner::MenuTree,
            viewport,
            trace_correlation(Some(session)),
            |ui| {
                for (menu_index, menu) in menus.iter().enumerate() {
                    let menu_selected =
                        session
                            .selection
                            .as_ref()
                            .is_some_and(|selection| match selection {
                                StableSelection::Menu(id) => id == &menu.id,
                                StableSelection::Ring { menu_id, .. }
                                | StableSelection::Cell { menu_id, .. } => menu_id == &menu.id,
                                StableSelection::CellSet(cells) => cells.primary.menu_id == menu.id,
                                _ => false,
                            });
                    let expansion_key = format!("menu:{}", menu.id);
                    let default_open = expanded_sections
                        .get(&expansion_key)
                        .copied()
                        // A collapsed tree is the compact default.  Once a
                        // section is explicitly opened or closed, its persisted
                        // value is authoritative and selection/diagnostics never
                        // force it open.
                        .unwrap_or(false);
                    let header = egui::CollapsingHeader::new(&menu.name)
                    .id_salt((
                        self.tree_widget_epoch,
                        menu::widget_key("menu", menu.id.as_str(), "tree"),
                    ))
                    .default_open(default_open)
                    .show(ui, |ui| {
                        for (ring_index, ring) in menu.rings.iter().enumerate() {
                            let ring_selection = StableSelection::Ring {
                                menu_id: menu.id.clone(),
                                ring_id: ring.id.clone(),
                            };
                            let ring_response = ui
                                .scope_builder(
                                    egui::UiBuilder::new().id(radial_native_selection_id(
                                        session,
                                        &ring_selection,
                                        "tree-ring",
                                    )),
                                    |ui| {
                                        ui.selectable_label(
                                            session.selection.as_ref() == Some(&ring_selection),
                                            format!("Ring {}", ring_index + 1),
                                        )
                                    },
                                )
                                .inner;
                            let ring_identity = radial_jump_selection_identity(&ring_selection);
                            let selected_by_jump = jump.as_ref().is_some_and(|jump| {
                                jump.register_select(
                                    ui,
                                    &ring_response,
                                    &format!("ring/{ring_identity}"),
                                    "Ring",
                                )
                            });
                            ring_response.clone().on_hover_text(format!(
                                "Ring {} · {}",
                                ring_index + 1,
                                ring.id
                            ));
                            if focus_restore.as_ref() == Some(&ring_selection) {
                                ring_response.request_focus();
                                *focus_restore = None;
                            }
                            if ring_response.clicked() || selected_by_jump {
                                queue_scope_change(
                                    pending_scope_change,
                                    session,
                                    ScopeChangeIntent::Navigation(DesignerNavigationIntent::Select {
                                        target: ring_selection,
                                        control: false,
                                        shift: false,
                                        force_history: false,
                                        open_properties: false,
                                    }),
                                );
                            }
                            ui.indent(
                                menu::widget_key(
                                    "ring",
                                    &format!("{}/{}", menu.id, ring.id),
                                    "cells",
                                ),
                                |ui| {
                                    for (index, cell) in ring.cells.iter().enumerate() {
                                        let selection = StableSelection::Cell {
                                            menu_id: menu.id.clone(),
                                            ring_id: ring.id.clone(),
                                            cell_id: cell.id.clone(),
                                        };
                                        let response = ui
                                            .scope_builder(
                                                egui::UiBuilder::new().id(radial_native_selection_id(
                                                    session,
                                                    &selection,
                                                    "tree-cell",
                                                )),
                                                |ui| {
                                                    let selected = session.selection.as_ref().is_some_and(|selection| {
                                                        selection.is_authored_cell_selected(
                                                            &crate::radial::authoring::AuthoredCellTarget {
                                                                menu_id: menu.id.clone(),
                                                                ring_id: ring.id.clone(),
                                                                cell_id: cell.id.clone(),
                                                            },
                                                        )
                                                    });
                                                    let accessible_name =
                                                        cell_tree_accessible_name(cell, index);
                                                    let response = ui
                                                        .selectable_label(selected, &cell.label)
                                                        .on_hover_text(if cell.label.is_empty() {
                                                            accessible_name.clone()
                                                        } else {
                                                            cell.label.clone()
                                                        });
                                                    response.widget_info(|| {
                                                        egui::WidgetInfo::selected(
                                                            egui::WidgetType::SelectableLabel,
                                                            ui.is_enabled(),
                                                            selected,
                                                            accessible_name.clone(),
                                                        )
                                                    });
                                                    response
                                                },
                                            )
                                            .inner;
                                        let cell_identity =
                                            radial_jump_selection_identity(&selection);
                                        let selected_by_jump = jump.as_ref().is_some_and(|jump| {
                                            jump.register_select(
                                                ui,
                                                &response,
                                                &format!("cell/{cell_identity}"),
                                                "Cell",
                                            )
                                        });
                                        if focus_restore
                                            .as_ref()
                                            .and_then(StableSelection::primary_cell)
                                            .as_ref()
                                            == Some(&selection)
                                        {
                                            response.request_focus();
                                            *focus_restore = None;
                                        }
                                        if response.clicked() || selected_by_jump {
                                            let modifiers = ui.input(|input| input.modifiers);
                                            queue_scope_change(
                                                pending_scope_change,
                                                session,
                                                ScopeChangeIntent::Navigation(DesignerNavigationIntent::Select {
                                                    target: StableSelection::Cell {
                                                        menu_id: menu.id.clone(),
                                                        ring_id: ring.id.clone(),
                                                        cell_id: cell.id.clone(),
                                                    },
                                                    control: modifiers.ctrl,
                                                    shift: modifiers.shift,
                                                    force_history: false,
                                                    open_properties: false,
                                                }),
                                            );
                                        }
                                        if response.drag_started() {
                                            *drag_source = Some((
                                                menu.id.clone(),
                                                ring.id.clone(),
                                                cell.id.clone(),
                                                session.generation,
                                            ));
                                        }
                                        if response.hovered()
                                            && ui.input(|input| input.pointer.any_released())
                                            && let Some((
                                                source_menu,
                                                source_ring,
                                                cell,
                                                expected_generation,
                                            )) = drag_source.take()
                                        {
                                            post_render.push(EditorCommand::MoveCell {
                                                source_menu,
                                                source_ring,
                                                cell,
                                                destination_menu: menu.id.clone(),
                                                destination_ring: ring.id.clone(),
                                                index,
                                                expected_generation,
                                            });
                                        }
                                    }
                                },
                            );
                        }
                    });
                    trace_designer_authoring_control(
                        ui,
                        &header.header_response,
                        DesignerAuthoringTarget::MenuRow,
                        DesignerAuthoringRole::Selectable,
                        Some(menu_index),
                        true,
                        menu_selected,
                        viewport,
                        trace_correlation(Some(session)),
                    );
                    if menu.id == session.draft.default_menu_id {
                        trace_designer_semantic_target(
                            ui,
                            &header.header_response,
                            DesignerSemanticTarget::DefaultMenu,
                            DesignerSemanticRole::Button,
                            viewport,
                            menu_selected,
                            trace_correlation(Some(session)),
                        );
                    }
                    let is_open = header.body_returned.is_some();
                    header
                        .header_response
                        .clone()
                        .on_hover_text(menu.name.clone());
                    let expansion_was_interacted_with = expanded_sections
                        .contains_key(&expansion_key)
                        || header.header_response.clicked();
                    if expansion_was_interacted_with
                        && expanded_sections.get(&expansion_key).copied() != Some(is_open)
                    {
                        expanded_sections.insert(expansion_key, is_open);
                        *preferences_dirty = true;
                    }
                    if focus_restore.as_ref() == Some(&StableSelection::Menu(menu.id.clone())) {
                        header.header_response.request_focus();
                        *focus_restore = None;
                    }
                    let menu_selection = StableSelection::Menu(menu.id.clone());
                    let menu_identity = radial_jump_selection_identity(&menu_selection);
                    let selected_by_jump = jump.as_ref().is_some_and(|jump| {
                        jump.register_select(
                            ui,
                            &header.header_response,
                            &format!("menu/{menu_identity}"),
                            "Menu",
                        )
                    });
                    if header.header_response.clicked()
                        || selected_by_jump
                        || menu_selected && session.selection.is_none()
                    {
                        queue_scope_change(
                            pending_scope_change,
                            session,
                            ScopeChangeIntent::Navigation(DesignerNavigationIntent::Select {
                                target: StableSelection::Menu(menu.id.clone()),
                                control: false,
                                shift: false,
                                force_history: false,
                                open_properties: false,
                            }),
                        );
                    }
                }
            },
        );
        secondary_help(
            ui,
            "Use the Design toolbar for New Menu, Add Ring, and slot-count proposals.",
        );
    }

    fn inspector(
        &mut self,
        ui: &mut egui::Ui,
        frame: &DesignerFrameContext,
        viewport: ViewportClass,
        ui_pass: &mut RadialUiPass,
    ) {
        let bulk_stage_scope = self.session.as_ref().and_then(|session| {
            matches!(
                session.selection.as_ref(),
                Some(StableSelection::CellSet(_))
            )
            .then(|| BulkEditStageScope {
                editor_session: session.editor_session,
                generation: session.generation,
                targets: session
                    .selection
                    .as_ref()
                    .map(StableSelection::selected_cells)
                    .unwrap_or_default(),
            })
        });
        self.reconcile_bulk_edit_stage(bulk_stage_scope);
        compact_section_heading(ui, "Inspector", None);
        if let Some(selection) = self.projected_selection.clone() {
            self.projected_selection_ui(ui, &selection);
            ui.separator();
        }
        let Some(selection) = self
            .session
            .as_ref()
            .and_then(|session| session.selection.clone())
        else {
            self.inspector_action_editor.invalidate_visit();
            self.inspector_action_editor_scope = None;
            secondary_help(ui, "Select a menu, ring, or cell.");
            return;
        };
        let selection_scope = ui_pass.jump.as_ref().map(|pass| {
            pass.child(&format!(
                "inspector/{}",
                radial_jump_selection_identity(&selection)
            ))
        });
        let parent_scope = std::mem::replace(&mut ui_pass.jump, selection_scope);
        match selection {
            StableSelection::Menu(menu_id) => {
                self.inspector_action_editor.invalidate_visit();
                self.inspector_action_editor_scope = None;
                self.menu_inspector(
                    ui,
                    menu_id,
                    frame.feature_defaults.default_menu_id.as_ref(),
                    viewport,
                    ui_pass,
                )
            }
            StableSelection::Ring { menu_id, ring_id } => {
                self.inspector_action_editor.invalidate_visit();
                self.inspector_action_editor_scope = None;
                self.ring_inspector(ui, menu_id, ring_id, ui_pass)
            }
            StableSelection::Cell {
                menu_id,
                ring_id,
                cell_id,
            } => self.cell_inspector(ui, frame, menu_id, ring_id, cell_id, ui_pass),
            StableSelection::CellSet(selection) => {
                self.bulk_cells_inspector(ui, selection, ui_pass)
            }
            _ => {
                self.inspector_action_editor.invalidate_visit();
                self.inspector_action_editor_scope = None;
                ui.label("This entity is edited by a later milestone.");
            }
        }
        ui_pass.jump = parent_scope;
    }

    fn menu_inspector(
        &mut self,
        ui: &mut egui::Ui,
        menu_id: MenuId,
        configured_default: Option<&MenuId>,
        viewport: ViewportClass,
        ui_pass: &mut RadialUiPass,
    ) {
        let jump = ui_pass.jump.clone();
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Some(found) = session.draft.menus.iter().find(|menu| menu.id == menu_id) else {
            return;
        };
        let previous_name = found.name.clone();
        let mut name = previous_name.clone();
        compact_section_heading(ui, "Menu", None);
        labeled_form_row(ui, "ID", |ui| ui.label(menu_id.to_string()));
        let response = ui
            .push_id(menu::widget_key("menu", menu_id.as_str(), "name"), |ui| {
                ui.text_edit_singleline(&mut name)
            })
            .inner;
        if let Some(jump) = jump.as_ref() {
            jump.register(
                ui,
                &response,
                &format!(
                    "menu/{:016x}/name",
                    radial_selection_digest(&StableSelection::Menu(menu_id.clone()))
                ),
                "Menu name",
                TargetKind::TextEntry,
            );
        }
        trace_designer_semantic_target(
            ui,
            &response,
            DesignerSemanticTarget::MenuName,
            DesignerSemanticRole::TextEdit,
            viewport,
            false,
            trace_correlation(Some(session)),
        );
        let widget_changed = response.changed();
        let submitted_name = (widget_changed && acceptance_trace::enabled()).then(|| name.clone());
        if widget_changed || response.lost_focus() || response.drag_stopped() {
            let phase = widget_edit_phase(&response);
            let _ = menu::rename_menu(session, menu_id.clone(), name, phase);
        }
        if let Some(submitted_name) = submitted_name {
            let current_name = session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == menu_id)
                .map(|menu| menu.name.as_str());
            acceptance_trace::emit_designer_edit_state(
                true,
                current_name.is_some_and(|name| name != previous_name),
                current_name == Some(submitted_name.as_str()),
                session.is_dirty(),
                trace_correlation(Some(session)),
            );
        }
        let menu_index = session
            .draft
            .menus
            .iter()
            .position(|menu| menu.id == menu_id)
            .unwrap_or(0);
        let current_skin = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .map(|menu| menu.skin_id.clone());
        if let Some(mut selected_skin) = current_skin {
            let combo_salt = menu::widget_key("menu", menu_id.as_str(), "default-skin");
            let combo_identity =
                format!("combo/{:016x}", ui.make_persistent_id(&combo_salt).value());
            let combo_jump = jump.as_ref().map(|jump| jump.combo_popup(&combo_identity));
            let default_skin = egui::ComboBox::from_id_salt(combo_salt)
                .selected_text(selected_skin.to_string())
                .show_ui(ui, |ui| {
                    for skin in &session.draft.skins {
                        let option = ui
                            .scope_builder(
                                egui::UiBuilder::new().id(egui::Id::new((
                                    "radial-skin-option",
                                    session.editor_session.0,
                                    radial_value_digest(&skin.id),
                                ))),
                                |ui| {
                                    ui.selectable_value(
                                        &mut selected_skin,
                                        skin.id.clone(),
                                        &skin.name,
                                    )
                                },
                            )
                            .inner;
                        register_radial_combo_option(
                            combo_jump.as_ref(),
                            ui,
                            &option,
                            &format!(
                                "{combo_identity}/option/{:016x}",
                                radial_value_digest(&skin.id)
                            ),
                            "Skin option",
                        );
                    }
                });
            register_radial_combo_anchor(
                ui_pass,
                &default_skin.response,
                &combo_identity,
                "Menu skin selector",
            );
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &default_skin.response,
                    &combo_identity,
                    "Menu skin selector",
                    TargetKind::ComboBox,
                );
            }
            ui_pass.record_combo(&default_skin.response);
            trace_designer_semantic_target(
                ui,
                &default_skin.response,
                DesignerSemanticTarget::MenuDefaultSkin,
                DesignerSemanticRole::ComboBox,
                viewport,
                false,
                trace_correlation(Some(session)),
            );
            let differs = session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == menu_id)
                .is_some_and(|menu| menu.skin_id != selected_skin);
            if differs {
                let mut document = (*session.draft).clone();
                if let Some(menu) = document.menus.iter_mut().find(|menu| menu.id == menu_id) {
                    menu.skin_id = selected_skin;
                    let _ = session.replace_document_atomic(document);
                }
            }
        }
        if let Some(mut edited) = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .cloned()
        {
            let original = edited.clone();
            let continuous = menu_behavior_controls(
                ui,
                &mut edited,
                &mut self.preferences.expanded_sections,
                trace_correlation(Some(session)),
                ui_pass,
            );
            if edited != original || continuous.is_some() {
                let mut document = (*session.draft).clone();
                if let Some(menu) = document.menus.iter_mut().find(|menu| menu.id == menu_id) {
                    *menu = edited;
                    if let Some(edit) = continuous {
                        let _ = dispatch_widget_document_edit(
                            session,
                            document,
                            format!("menu:{menu_id}"),
                            &edit.field,
                            edit.signals,
                        );
                    } else {
                        let _ = session.replace_document_atomic(document);
                    }
                }
            }
        }
        ui.horizontal(|ui| {
            let move_up = ui.add_enabled(menu_index > 0, egui::Button::new("Move up"));
            if let Some(jump) = jump.as_ref() {
                jump.register(ui, &move_up, "move-up", "Move menu up", TargetKind::Button);
            }
            if move_up.clicked()
                && menu::move_menu(session, &menu_id, menu_index.saturating_sub(1)).is_ok()
            {
                self.focus_restore = session.selection.clone();
            }
            let move_down = ui.button("Move down");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &move_down,
                    "move-down",
                    "Move menu down",
                    TargetKind::Button,
                );
            }
            if move_down.clicked() && menu::move_menu(session, &menu_id, menu_index + 1).is_ok() {
                self.focus_restore = session.selection.clone();
            }
        });
        let duplicate_link = ui.button("Duplicate (link submenus)");
        if let Some(jump) = jump.as_ref() {
            jump.register(
                ui,
                &duplicate_link,
                "duplicate-link-submenus",
                "Duplicate menu with linked submenus",
                TargetKind::Button,
            );
        }
        if duplicate_link.clicked() {
            let _ = menu::duplicate_menu(session, &menu_id, SubmenuDuplication::LinkExisting);
        }
        let duplicate_subtree = ui.button("Duplicate subtree");
        if let Some(jump) = jump.as_ref() {
            jump.register(
                ui,
                &duplicate_subtree,
                "duplicate-subtree",
                "Duplicate menu subtree",
                TargetKind::Button,
            );
        }
        if duplicate_subtree.clicked() {
            let _ =
                menu::duplicate_menu(session, &menu_id, SubmenuDuplication::CloneSubmenuClosure);
        }
        let configured_default = configured_default == Some(&menu_id);
        let delete = ui
            .add_enabled(!configured_default, egui::Button::new("Delete menu"))
            .on_disabled_hover_text("This menu is selected as the default in Settings");
        if let Some(jump) = jump.as_ref() {
            jump.register(ui, &delete, "delete", "Delete menu", TargetKind::Button);
        }
        if with_button_emphasis(ui, delete, ButtonEmphasis::Destructive).clicked()
            && let Err(error) = menu::delete_menu(session, &menu_id)
        {
            self.delete_message = Some(format!("Delete blocked: {error:?}"));
        }
    }

    fn projected_selection_ui(&mut self, ui: &mut egui::Ui, selection: &ProjectedSelection) {
        let ProjectedCellProvenance::Dynamic {
            menu_id,
            ring_id,
            source_cell_id,
            source,
            result_index,
            ..
        } = &selection.provenance
        else {
            return;
        };
        ui.group(|ui| {
            ui.label("Generated preview result (read-only)");
            ui.label(&selection.label);
            ui.label(format!("Source: {source:?} · result {result_index}"));
            let source_selection = self.session.as_ref().and_then(|session| {
                session
                    .draft
                    .menus
                    .iter()
                    .find(|menu| &menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
                    .and_then(|ring| {
                        ring.cells.iter().find_map(|cell| {
                            (cell.id == *source_cell_id
                                && matches!(&cell.content, CellContent::Dynamic { .. }))
                            .then(|| StableSelection::Cell {
                                menu_id: menu_id.clone(),
                                ring_id: ring_id.clone(),
                                cell_id: cell.id.clone(),
                            })
                        })
                    })
            });
            if let Some(source_selection) = source_selection {
                let edit_source = ui.button("Edit dynamic source definition");
                if let Some(session) = self.session.as_ref() {
                    trace_designer_authoring_control_with_scope(
                        ui,
                        &edit_source,
                        DesignerAuthoringTarget::EditDynamicSource,
                        DesignerAuthoringRole::Button,
                        None,
                        true,
                        false,
                        ViewportClass::Deferred,
                        trace_correlation(Some(session)),
                        acceptance_trace::DesignerAuthoringControlScope::DynamicSourceEdit(
                            acceptance_trace::DesignerDynamicSourceEditScope {
                                source_target_digest: acceptance_identity_digest(&[
                                    menu_id.as_str(),
                                    ring_id.as_str(),
                                    source_cell_id.as_str(),
                                ]),
                                result_index: *result_index,
                            },
                        ),
                    );
                }
                if edit_source.clicked() {
                    self.queue_navigation_intent(DesignerNavigationIntent::Select {
                        target: source_selection,
                        control: false,
                        shift: false,
                        force_history: false,
                        open_properties: false,
                    });
                }
            } else {
                ui.small("The authored source definition is not present in this menu.");
            }
        });
    }

    fn ring_inspector(
        &mut self,
        ui: &mut egui::Ui,
        menu_id: MenuId,
        ring_id: RingId,
        ui_pass: &mut RadialUiPass,
    ) {
        let jump = ui_pass.jump.clone();
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let count = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
            .map_or(0, |ring| ring.cells.len());
        compact_section_heading(ui, "Ring layout", None);
        labeled_form_row(ui, "ID", |ui| ui.label(ring_id.to_string()));
        labeled_form_row(ui, "Cells", |ui| ui.label(count.to_string()));
        if let Some(mut edited) = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
            .cloned()
        {
            let radius_response = ui.add(
                egui::DragValue::new(&mut edited.radius)
                    .prefix("Radius ")
                    .speed(1.0),
            );
            let cell_radius_response = ui.add(
                egui::DragValue::new(&mut edited.cell_radius)
                    .prefix("Cell radius ")
                    .speed(1.0),
            );
            let rotation_response = ui.add(
                egui::DragValue::new(&mut edited.rotation_degrees)
                    .prefix("Rotation ° ")
                    .speed(1.0),
            );
            let gap_response = ui.add(
                egui::DragValue::new(&mut edited.gap)
                    .prefix("Gap ")
                    .speed(0.5),
            );
            if let Some(jump) = jump.as_ref() {
                for (identity, label, response) in [
                    ("radius", "Ring radius", &radius_response),
                    ("cell-radius", "Cell radius", &cell_radius_response),
                    ("rotation", "Ring rotation", &rotation_response),
                    ("gap", "Ring gap", &gap_response),
                ] {
                    jump.register(ui, response, identity, label, TargetKind::Slider);
                }
            }
            let edit = [
                ("radius", radius_response),
                ("cell_radius", cell_radius_response),
                ("rotation_degrees", rotation_response),
                ("gap", gap_response),
            ]
            .into_iter()
            .find(|(_, response)| {
                response.changed() || response.lost_focus() || response.drag_stopped()
            });
            if let Some((field, response)) = edit {
                let mut document = (*session.draft).clone();
                if let Some(ring) = document
                    .menus
                    .iter_mut()
                    .find(|menu| menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter_mut().find(|ring| ring.id == ring_id))
                {
                    *ring = edited;
                    let _ = dispatch_widget_document_edit(
                        session,
                        document,
                        format!("ring:{menu_id}/{ring_id}"),
                        field,
                        WidgetEditSignals::from_response(&response),
                    );
                }
            }
        }
        let ring_index = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().position(|ring| ring.id == ring_id))
            .unwrap_or(0);
        ui.horizontal(|ui| {
            let move_inward = ui.add_enabled(ring_index > 0, egui::Button::new("Move inward"));
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &move_inward,
                    "move-inward",
                    "Move ring inward",
                    TargetKind::Button,
                );
            }
            if move_inward.clicked()
                && menu::move_ring(session, &menu_id, &ring_id, ring_index.saturating_sub(1))
                    .is_ok()
            {
                self.focus_restore = session.selection.clone();
            }
            let move_outward = ui.button("Move outward");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &move_outward,
                    "move-outward",
                    "Move ring outward",
                    TargetKind::Button,
                );
            }
            if move_outward.clicked()
                && menu::move_ring(session, &menu_id, &ring_id, ring_index + 1).is_ok()
            {
                self.focus_restore = session.selection.clone();
            }
        });
        secondary_help(
            ui,
            "Change authored slot count through the proposal controls above the board.",
        );
        let delete = ui.button("Delete ring");
        if let Some(jump) = jump.as_ref() {
            jump.register(ui, &delete, "delete", "Delete ring", TargetKind::Button);
        }
        if with_button_emphasis(ui, delete, ButtonEmphasis::Destructive).clicked() {
            if count == 0 {
                if let Err(error) = menu::delete_ring(session, &menu_id, &ring_id) {
                    session.last_error = Some(format!("Delete ring failed: {error:?}"));
                }
            } else {
                self.delete_ring_prompt = Some((menu_id, ring_id));
            }
        }
    }

    fn bulk_cells_inspector(
        &mut self,
        ui: &mut egui::Ui,
        selection: crate::radial::authoring::AuthoredCellSelection,
        ui_pass: &mut RadialUiPass,
    ) {
        let jump = ui_pass.jump.clone();
        let Some((document, correlation, editor_session, generation)) =
            self.session.as_ref().map(|session| {
                (
                    (*session.draft).clone(),
                    trace_correlation(Some(session)),
                    session.editor_session,
                    session.generation,
                )
            })
        else {
            return;
        };
        let targets = selection.members.clone();
        if targets.is_empty() {
            ui.label("Select authored cells to edit them together.");
            return;
        }
        ui.label(format!("{} authored cells selected", targets.len()));
        ui.small("Bulk selection stays within one menu. Ring order and cell order define ranges.");
        let labels = targets
            .iter()
            .filter_map(|target| {
                document
                    .menus
                    .iter()
                    .find(|menu| menu.id == target.menu_id)
                    .and_then(|menu| menu.rings.iter().find(|ring| ring.id == target.ring_id))
                    .and_then(|ring| ring.cells.iter().find(|cell| cell.id == target.cell_id))
                    .map(|cell| cell.label.clone())
            })
            .collect::<Vec<_>>();
        let labels_are_mixed = labels.windows(2).any(|pair| pair[0] != pair[1]);
        ui.horizontal_wrapped(|ui| {
            ui.label(if labels_are_mixed {
                "Mixed labels"
            } else {
                "Set label for every selected cell"
            });
            let label_response = ui.text_edit_singleline(&mut self.bulk_label_value);
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &label_response,
                    "label",
                    "Selected cell labels",
                    TargetKind::TextEntry,
                );
            }
            trace_designer_authoring_control(
                ui,
                &label_response,
                DesignerAuthoringTarget::BulkLabel,
                DesignerAuthoringRole::TextEdit,
                None,
                true,
                false,
                ViewportClass::Deferred,
                correlation,
            );
            let apply_label = ui.button(format!("Set label on {} cells", targets.len()));
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &apply_label,
                    "set-label",
                    "Set labels on selected cells",
                    TargetKind::Button,
                );
            }
            trace_designer_authoring_control(
                ui,
                &apply_label,
                DesignerAuthoringTarget::BulkSetLabel,
                DesignerAuthoringRole::Button,
                None,
                true,
                false,
                ViewportClass::Deferred,
                correlation,
            );
            if apply_label.clicked() {
                self.commit_bulk_edit(
                    &targets,
                    BulkCellEdit::SetLabel(self.bulk_label_value.clone()),
                );
            }
        });

        let primary = &targets[0];
        let style_scope = skin_editor::StyleScope::Cell {
            menu_id: primary.menu_id.clone(),
            ring_id: primary.ring_id.clone(),
            cell_id: primary.cell_id.clone(),
        };
        let style_rows = skin_editor::rows(&document, &style_scope).unwrap_or_default();
        let fields = style_rows
            .iter()
            .filter_map(|row| {
                let field = skin_editor::style_field(&row.section, &row.field)?;
                (menu::supports_bulk_cell_style_field(field)
                    && skin_editor::control_kind(&row.section, &row.field).is_some())
                .then(|| (row.section.clone(), row.field.clone(), field))
            })
            .collect::<Vec<_>>();
        if !fields.is_empty() {
            if !fields
                .iter()
                .any(|(section, field, _)| self.bulk_style_field == format!("{section}.{field}"))
            {
                self.bulk_style_field = format!("{}.{}", fields[0].0, fields[0].1);
                self.bulk_style_staged_value = None;
                self.bulk_style_text.clear();
            }
            ui.separator();
            ui.label("One style field");
            let selected_field_key = self.bulk_style_field.clone();
            let style_combo_identity = format!(
                "combo/{:016x}",
                ui.make_persistent_id("radial-bulk-style-field").value()
            );
            let style_combo_jump = jump
                .as_ref()
                .map(|jump| jump.combo_popup(&style_combo_identity));
            let style_field_combo = egui::ComboBox::from_id_salt("radial-bulk-style-field")
                .selected_text(&selected_field_key)
                .show_ui(ui, |ui| {
                    for (section, field, _) in &fields {
                        let key = format!("{section}.{field}");
                        let option = ui
                            .scope_builder(
                                egui::UiBuilder::new().id(ui
                                    .id()
                                    .with(("radial-bulk-style-option", radial_value_digest(&key)))),
                                |ui| {
                                    ui.selectable_value(
                                        &mut self.bulk_style_field,
                                        key.clone(),
                                        &key,
                                    )
                                },
                            )
                            .inner;
                        register_radial_combo_option(
                            style_combo_jump.as_ref(),
                            ui,
                            &option,
                            &format!(
                                "{style_combo_identity}/option/{:016x}",
                                radial_value_digest(&key)
                            ),
                            "Style field option",
                        );
                        if option.changed() {
                            self.bulk_style_stage_key.clear();
                        }
                    }
                });
            ui_pass.record_combo(&style_field_combo.response);
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &style_field_combo.response,
                    &style_combo_identity,
                    "Bulk style field selector",
                    TargetKind::ComboBox,
                );
            }
            let Some((section, field, style_field)) = fields
                .iter()
                .find(|(section, field, _)| self.bulk_style_field == format!("{section}.{field}"))
                .cloned()
            else {
                return;
            };
            let stage_key = format!(
                "{:?}|{:?}|{}|{}|{}",
                editor_session,
                generation,
                targets
                    .iter()
                    .map(|target| format!(
                        "{}/{}/{}",
                        target.menu_id, target.ring_id, target.cell_id
                    ))
                    .collect::<Vec<_>>()
                    .join(","),
                section,
                field,
            );
            if self.bulk_style_stage_key != stage_key {
                self.bulk_style_stage_key = stage_key;
                self.bulk_style_staged_value = None;
                self.bulk_style_text.clear();
            }
            let style_values = targets
                .iter()
                .filter_map(|target| {
                    let cell = document
                        .menus
                        .iter()
                        .find(|menu| menu.id == target.menu_id)?
                        .rings
                        .iter()
                        .find(|ring| ring.id == target.ring_id)?
                        .cells
                        .iter()
                        .find(|cell| cell.id == target.cell_id)?;
                    serde_json::to_value(&cell.style)
                        .ok()?
                        .get(&section)?
                        .get(&field)
                        .cloned()
                })
                .collect::<Vec<_>>();
            let style_is_mixed = style_values.windows(2).any(|pair| pair[0] != pair[1]);
            let primary_row = style_rows
                .iter()
                .find(|row| row.section == section && row.field == field);
            if let Some(row) = primary_row {
                ui.label(if style_is_mixed {
                    "Mixed style override · set a new value for this field"
                } else {
                    "Same style override on all selected cells"
                });
                let mode_before = self.bulk_style_mode;
                enum_combo(
                    ui,
                    "Override",
                    &mut self.bulk_style_mode,
                    &[
                        ("Set value", BulkStyleMode::Set),
                        ("Inherit", BulkStyleMode::Inherit),
                        ("Clear", BulkStyleMode::Clear),
                    ],
                    ui_pass,
                );
                if self.bulk_style_mode != mode_before {
                    self.bulk_style_staged_value = match self.bulk_style_mode {
                        BulkStyleMode::Inherit => Some(skin_editor::inherit_value()),
                        BulkStyleMode::Clear => Some(skin_editor::clear_value()),
                        BulkStyleMode::Set => None,
                    };
                }
                if self.bulk_style_mode == BulkStyleMode::Set {
                    let kind = skin_editor::control_kind(&section, &field);
                    if kind == Some(skin_editor::StyleControlKind::Resource) {
                        let media_kind = if section == "sounds" {
                            crate::radial::model::MediaKind::Sound
                        } else {
                            crate::radial::model::MediaKind::Image
                        };
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Managed file…").clicked() {
                                self.enqueue_bulk_style_media(
                                    &targets,
                                    style_field,
                                    media_kind,
                                    true,
                                );
                            }
                            if ui.button("External file…").clicked() {
                                self.enqueue_bulk_style_media(
                                    &targets,
                                    style_field,
                                    media_kind,
                                    false,
                                );
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.text_edit_singleline(&mut self.bulk_style_text);
                            if ui.button("Set search-path value").clicked()
                                && !self.bulk_style_text.trim().is_empty()
                            {
                                let value = crate::radial::model::Override::Value(
                                    crate::radial::model::MediaReference::SearchPath {
                                        file_name: self.bulk_style_text.trim().to_owned(),
                                    },
                                );
                                match serde_json::to_value(value) {
                                    Ok(value) => {
                                        self.commit_bulk_style(&targets, style_field, value);
                                    }
                                    Err(error) => self.bulk_feedback = Some(error.to_string()),
                                }
                            }
                        });
                        if section == "images" {
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("Set icon resource").clicked()
                                    && !self.bulk_style_text.trim().is_empty()
                                {
                                    let value = crate::radial::model::Override::Value(
                                        crate::radial::model::MediaReference::IconResource {
                                            path: self.bulk_style_text.trim().to_owned(),
                                            index: 1,
                                        },
                                    );
                                    match serde_json::to_value(value) {
                                        Ok(value) => {
                                            self.commit_bulk_style(&targets, style_field, value);
                                        }
                                        Err(error) => self.bulk_feedback = Some(error.to_string()),
                                    }
                                }
                            });
                        }
                    } else if let Some(kind) = kind {
                        let current = self.bulk_style_staged_value.clone().unwrap_or_else(|| {
                            if style_is_mixed {
                                skin_editor::inherit_value()
                            } else {
                                style_values
                                    .first()
                                    .cloned()
                                    .unwrap_or_else(skin_editor::inherit_value)
                            }
                        });
                        if let Some((value, _)) = style_value_widget(
                            ui,
                            kind,
                            &current,
                            &row.inherited,
                            &mut self.bulk_style_text,
                            &[],
                            &format!("{section}.{field}"),
                            None,
                            Some(ui_pass),
                        ) {
                            self.bulk_style_staged_value = Some(value);
                        }
                    }
                }
                if self.bulk_style_mode != BulkStyleMode::Set {
                    self.bulk_style_staged_value = Some(match self.bulk_style_mode {
                        BulkStyleMode::Inherit => skin_editor::inherit_value(),
                        BulkStyleMode::Clear => skin_editor::clear_value(),
                        BulkStyleMode::Set => unreachable!(),
                    });
                }
                if ui
                    .add_enabled(
                        self.bulk_style_staged_value.is_some()
                            && self.bulk_style_mode == BulkStyleMode::Set,
                        egui::Button::new(format!(
                            "Apply {section}.{field} to {} cells",
                            targets.len()
                        )),
                    )
                    .clicked()
                    && let Some(value) = self.bulk_style_staged_value.clone()
                {
                    self.commit_bulk_style(&targets, style_field, value);
                }
                if self.bulk_style_mode != BulkStyleMode::Set
                    && ui
                        .button(format!("Apply override choice to {} cells", targets.len()))
                        .clicked()
                    && let Some(value) = self.bulk_style_staged_value.clone()
                {
                    self.commit_bulk_style(&targets, style_field, value);
                }
            }
        }

        ui.separator();
        ui.label("After-action policy");
        let policies = targets
            .iter()
            .filter_map(|target| {
                let cell = document
                    .menus
                    .iter()
                    .find(|menu| menu.id == target.menu_id)?
                    .rings
                    .iter()
                    .find(|ring| ring.id == target.ring_id)?
                    .cells
                    .iter()
                    .find(|cell| cell.id == target.cell_id)?;
                Some(match self.bulk_after_action_slot {
                    BulkAfterActionSlot::Primary => cell.after_action,
                    BulkAfterActionSlot::Secondary => cell.secondary_after_action,
                })
            })
            .collect::<Vec<_>>();
        let policy_is_mixed = policies.windows(2).any(|pair| pair[0] != pair[1]);
        ui.horizontal_wrapped(|ui| {
            enum_combo(
                ui,
                "Slot",
                &mut self.bulk_after_action_slot,
                &[
                    ("Primary", BulkAfterActionSlot::Primary),
                    ("Secondary", BulkAfterActionSlot::Secondary),
                ],
                ui_pass,
            );
            enum_combo(
                ui,
                "New policy",
                &mut self.bulk_after_action_policy,
                &AFTER_ACTION_CHOICES,
                ui_pass,
            );
        });
        ui.label(if policy_is_mixed {
            "Mixed policies · applying sets only the chosen slot"
        } else {
            "Same policy on all selected cells"
        });
        if ui
            .button(format!("Set policy on {} cells", targets.len()))
            .clicked()
        {
            self.commit_bulk_edit(
                &targets,
                BulkCellEdit::SetAfterAction {
                    slot: self.bulk_after_action_slot,
                    policy: self.bulk_after_action_policy,
                },
            );
        }
        if let Some(feedback) = self.bulk_feedback.as_deref() {
            ui.colored_label(ui.visuals().error_fg_color, feedback);
        }
    }

    fn commit_bulk_edit(
        &mut self,
        targets: &[crate::radial::authoring::AuthoredCellTarget],
        edit: BulkCellEdit,
    ) {
        let result = self.session.as_mut().map_or_else(
            || Err(menu::BulkCellEditError::StaleSession),
            |session| {
                let request = BulkCellEditRequest::capture(session)?;
                if request.targets != targets {
                    return Err(menu::BulkCellEditError::StaleGeneration);
                }
                let assets = session.pending_assets.clone();
                menu::apply_bulk_cell_edit_with_assets(session, &request, edit, assets, Vec::new())
            },
        );
        self.bulk_feedback = Some(match result {
            Ok(()) => format!("Applied one atomic edit to {} cells.", targets.len()),
            Err(error) => format!("Bulk edit rejected: {error:?}"),
        });
    }

    fn commit_bulk_style(
        &mut self,
        targets: &[crate::radial::authoring::AuthoredCellTarget],
        field: crate::radial::skin::StyleField,
        value: serde_json::Value,
    ) {
        self.commit_bulk_edit(targets, BulkCellEdit::SetStyleOverride { field, value });
        self.bulk_style_staged_value = None;
    }

    fn enqueue_bulk_style_media(
        &mut self,
        targets: &[crate::radial::authoring::AuthoredCellTarget],
        field: crate::radial::skin::StyleField,
        media_kind: crate::radial::model::MediaKind,
        managed: bool,
    ) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        match BulkCellEditRequest::capture(session) {
            Ok(request) if request.targets == targets => {
                self.intent_bridge
                    .enqueue_file_dialog(DesignerFileDialogRequest::BulkStyleMedia {
                        request,
                        field,
                        media_kind,
                        managed,
                    })
            }
            Ok(_) => {
                self.bulk_feedback = Some("Selection changed before media selection began.".into())
            }
            Err(error) => self.bulk_feedback = Some(format!("Cannot select media: {error:?}")),
        }
    }

    fn cell_inspector(
        &mut self,
        ui: &mut egui::Ui,
        frame: &DesignerFrameContext,
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
        ui_pass: &mut RadialUiPass,
    ) {
        let jump = ui_pass.jump.clone();
        let invocation_context = self
            .session
            .as_ref()
            .and_then(|session| session.sampled_preview_context.clone())
            .unwrap_or_else(|| InvocationContext::empty(0));
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let mut deferred_operation = None;
        let Some(cell) = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
            .and_then(|ring| ring.cells.iter().find(|cell| cell.id == cell_id))
            .cloned()
        else {
            return;
        };
        let cell_index = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| {
                let mut offset = 0usize;
                for ring in &menu.rings {
                    if ring.id == ring_id {
                        return ring
                            .cells
                            .iter()
                            .position(|candidate| candidate.id == cell_id)
                            .map(|index| offset.saturating_add(index));
                    }
                    offset = offset.saturating_add(ring.cells.len());
                }
                None
            });
        let id_label = ui.label(format!("ID: {}", cell_id));
        if let Some(cell_index) = cell_index {
            trace_designer_authoring_control(
                ui,
                &id_label,
                DesignerAuthoringTarget::InspectorCell,
                DesignerAuthoringRole::Region,
                Some(cell_index),
                true,
                true,
                ViewportClass::Deferred,
                trace_correlation(Some(session)),
            );
        }
        if self.placement_draft.as_ref().is_some_and(|draft| {
            draft.menu_id == menu_id && draft.ring_id == ring_id && draft.cell_id == cell_id
        }) {
            ui.group(|ui| {
                ui.label("Placement draft");
                ui.small("Choose valid content below to commit this empty authored slot.");
                if ui.button("Cancel placement").clicked() {
                    self.placement_draft = None;
                }
            });
        }
        let mut label = cell.label.clone();
        let label_response = ui
            .push_id(
                menu::widget_key("cell", &format!("{menu_id}/{ring_id}/{cell_id}"), "label"),
                |ui| ui.text_edit_singleline(&mut label),
            )
            .inner;
        if let Some(jump) = jump.as_ref() {
            jump.register(
                ui,
                &label_response,
                "label",
                "Cell label",
                TargetKind::TextEntry,
            );
        }
        if label_response.changed() || label_response.lost_focus() || label_response.drag_stopped()
        {
            let _ = menu::set_cell_label(
                session,
                &menu_id,
                &ring_id,
                &cell_id,
                label.clone(),
                widget_edit_phase(&label_response),
            );
        }
        // Publish the current rendered edit target as well as edits and focus
        // transitions. The acceptance runner uses this real widget receipt to
        // acquire and click the Inspector field; it cannot rely on a native UIA
        // provider for the deferred egui viewport.
        trace_inspector_cell_text_edit(
            ui,
            &label_response,
            &menu_id,
            &ring_id,
            &cell_id,
            session.editor_session.0,
            session.generation.0,
            &label,
        );
        if let Some(mut edited) = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
            .and_then(|ring| ring.cells.iter().find(|candidate| candidate.id == cell_id))
            .cloned()
        {
            let original = edited.clone();
            after_action_combo(
                ui,
                "Primary after action",
                &mut edited.after_action,
                ui_pass,
            );
            after_action_combo(
                ui,
                "Secondary after action",
                &mut edited.secondary_after_action,
                ui_pass,
            );
            let content_edit = cell_content_controls(
                ui,
                &session.draft,
                &mut edited,
                &mut self.preferences.expanded_sections,
                ui_pass,
            );
            let media_edit = cell_media_controls(
                ui,
                &session.draft,
                &mut edited,
                &self.intent_bridge,
                &mut self.preferences.expanded_sections,
                &menu_id,
                &ring_id,
                &cell_id,
                ui_pass,
            );
            let trigger_edit = cell_trigger_controls(
                ui,
                session,
                &mut edited,
                &mut self.preferences.expanded_sections,
                ui_pass,
            );
            let continuous = content_edit.or(media_edit).or(trigger_edit);
            if primary_content_replaced(&original.content, &edited.content) {
                deferred_operation = Some(DeferredEditorOperation::SetCellContent {
                    menu_id: menu_id.clone(),
                    ring_id: ring_id.clone(),
                    cell_id: cell_id.clone(),
                    content: edited.content.clone(),
                });
            } else if edited != original || continuous.is_some() {
                let mut document = (*session.draft).clone();
                if let Some(slot) = document
                    .menus
                    .iter_mut()
                    .find(|menu| menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter_mut().find(|ring| ring.id == ring_id))
                    .and_then(|ring| {
                        ring.cells
                            .iter_mut()
                            .find(|candidate| candidate.id == cell_id)
                    })
                {
                    let completes_placement = matches!(&original.content, CellContent::Spacer)
                        && !matches!(&edited.content, CellContent::Spacer);
                    *slot = edited;
                    let policy_issue = document
                        .menus
                        .iter()
                        .find(|menu| menu.id == menu_id)
                        .and_then(|candidate_menu| {
                            let candidate_cell = candidate_menu
                                .rings
                                .iter()
                                .find(|ring| ring.id == ring_id)
                                .and_then(|ring| {
                                    ring.cells.iter().find(|candidate| candidate.id == cell_id)
                                })?;
                            let primary_touched = original.after_action != candidate_cell.after_action
                                || original.content != candidate_cell.content;
                            let secondary_touched = original.secondary_after_action
                                != candidate_cell.secondary_after_action
                                || original.alternate_clicks != candidate_cell.alternate_clicks;
                            primary_touched
                                .then(|| {
                                    crate::radial::validation::cell_policy_incompatibility(
                                        &document,
                                        candidate_menu,
                                        candidate_cell,
                                        crate::radial::validation::CellAfterActionSlot::Primary,
                                        candidate_cell.after_action,
                                    )
                                })
                                .flatten()
                                .or_else(|| {
                                    secondary_touched.then(|| {
                                        crate::radial::validation::cell_policy_incompatibility(
                                            &document,
                                            candidate_menu,
                                            candidate_cell,
                                            crate::radial::validation::CellAfterActionSlot::Secondary,
                                            candidate_cell.secondary_after_action,
                                        )
                                    })
                                    .flatten()
                                })
                        });
                    if let Some(requirement) = policy_issue {
                        self.delete_message = Some(format!(
                            "Cell edit blocked: KeepOpen is incompatible with {requirement:?}"
                        ));
                    } else if let Err(error) = menu::validate_submenu_graph(&document) {
                        self.delete_message = Some(format!("Cell edit blocked: {error:?}"));
                    } else {
                        if let Some(edit) = continuous {
                            let _ = dispatch_widget_document_edit(
                                session,
                                document,
                                format!("cell:{menu_id}/{ring_id}/{cell_id}"),
                                &edit.field,
                                edit.signals,
                            );
                        } else {
                            let _ = session.replace_document_atomic(document);
                        }
                        if completes_placement {
                            self.placement_draft = None;
                        }
                    }
                }
            }
        }
        if matches!(&cell.content, CellContent::Spacer) {
            ui.separator();
            ui.label("Empty authored slot");
            ui.text_edit_singleline(&mut self.submenu_name);
            if ui.button("Create and link new submenu").clicked() {
                deferred_operation = Some(DeferredEditorOperation::CreateSubmenuAndLink {
                    menu_id: menu_id.clone(),
                    ring_id: ring_id.clone(),
                    cell_id: cell_id.clone(),
                    name: self.submenu_name.clone(),
                });
            }
            let mut linked_menu = self.submenu_link_target.clone();
            if linked_menu
                .as_ref()
                .is_some_and(|id| !session.draft.menus.iter().any(|menu| &menu.id == id))
            {
                linked_menu = None;
            }
            let submenu_combo = egui::ComboBox::from_label("Link existing submenu")
                .selected_text(
                    linked_menu
                        .as_ref()
                        .and_then(|id| session.draft.menus.iter().find(|menu| &menu.id == id))
                        .map_or_else(|| "Choose menu".into(), |menu| menu.name.clone()),
                )
                .show_ui(ui, |ui| {
                    for menu in &session.draft.menus {
                        if menu.id != menu_id {
                            ui.selectable_value(
                                &mut linked_menu,
                                Some(menu.id.clone()),
                                &menu.name,
                            );
                        }
                    }
                });
            ui_pass.record_combo(&submenu_combo.response);
            self.submenu_link_target = linked_menu.clone();
            if ui
                .add_enabled(linked_menu.is_some(), egui::Button::new("Link existing"))
                .clicked()
                && let Some(linked_menu) = linked_menu
            {
                deferred_operation = Some(DeferredEditorOperation::LinkExistingSubmenu {
                    menu_id: menu_id.clone(),
                    ring_id: ring_id.clone(),
                    cell_id: cell_id.clone(),
                    linked_menu,
                });
            }
        }
        if ui.button("Copy cell").clicked() {
            deferred_operation = Some(DeferredEditorOperation::CopyCell {
                menu_id: menu_id.clone(),
                ring_id: ring_id.clone(),
                cell_id: cell_id.clone(),
            });
        }
        if let CellContent::Submenu { menu_id: linked } = &cell.content
            && ui.button("Clone reusable submenu graph").clicked()
        {
            deferred_operation = Some(DeferredEditorOperation::CloneSubmenuAndLink {
                menu_id: menu_id.clone(),
                ring_id: ring_id.clone(),
                cell_id: cell_id.clone(),
                linked_menu: linked.clone(),
            });
        }
        if ui.button("Make spacer").clicked() {
            deferred_operation = Some(DeferredEditorOperation::SetCellContent {
                menu_id: menu_id.clone(),
                ring_id: ring_id.clone(),
                cell_id: cell_id.clone(),
                content: CellContent::Spacer,
            });
        }
        let delete = ui.button("Delete cell");
        if with_button_emphasis(ui, delete, ButtonEmphasis::Destructive).clicked() {
            deferred_operation = Some(DeferredEditorOperation::DeleteCell {
                menu_id: menu_id.clone(),
                ring_id: ring_id.clone(),
                cell_id: cell_id.clone(),
            });
        }
        ui.separator();
        ui.label("Universal Action");
        let scope = BindingEditorScope {
            surface: BindingEditorSurface::Inspector,
            editor_session: session.editor_session,
            draft_generation: session.generation,
            target: StableSelection::Cell {
                menu_id: menu_id.clone(),
                ring_id: ring_id.clone(),
                cell_id: cell_id.clone(),
            },
            slot: BindingEditorSlot::CellPrimary,
        };
        self.inspector_action_editor_scope = Some(scope.clone());
        let assigned_binding = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
            .and_then(|ring| ring.cells.iter().find(|cell| cell.id == cell_id))
            .and_then(|cell| match &cell.content {
                CellContent::Action { binding } => Some(binding.clone()),
                _ => None,
            });
        let editor_intents = self
            .inspector_action_editor
            .show_with_action_catalog_and_jump(
                ui,
                scope,
                assigned_binding.as_ref(),
                &invocation_context,
                Some(frame.action_catalog.as_ref()),
                Some(ui_pass),
            );
        for intent in editor_intents {
            match intent {
                ActionBindingEditorIntent::Pin { binding } => {
                    let result = menu::set_cell_content(
                        session,
                        &menu_id,
                        &ring_id,
                        &cell_id,
                        CellContent::Action {
                            binding: binding.clone(),
                        },
                    );
                    if let Err(error) = self.inspector_action_editor.finish_assignment(
                        binding,
                        BindingAssignmentKind::Pinned,
                        result,
                    ) {
                        self.delete_message = Some(format!("Action assignment failed: {error:?}"));
                    } else {
                        self.placement_draft = None;
                    }
                }
                ActionBindingEditorIntent::SaveQuery { binding }
                | ActionBindingEditorIntent::SetCommand { binding } => {
                    let result = menu::set_cell_content(
                        session,
                        &menu_id,
                        &ring_id,
                        &cell_id,
                        CellContent::Action {
                            binding: binding.clone(),
                        },
                    );
                    if let Err(error) = self.inspector_action_editor.finish_assignment(
                        binding,
                        BindingAssignmentKind::Explicit,
                        result,
                    ) {
                        self.delete_message = Some(format!("Action assignment failed: {error:?}"));
                    } else {
                        self.placement_draft = None;
                    }
                }
                ActionBindingEditorIntent::Search { identity, query } => {
                    frame
                        .intent_bridge
                        .push(DesignerUiIntent::ActionSearch { identity, query });
                }
                ActionBindingEditorIntent::Test {
                    identity,
                    binding,
                    history_query,
                } => frame.intent_bridge.push(DesignerUiIntent::TestAction {
                    identity,
                    payload: Box::new(crate::gui::AuthoringProviderTestPayload {
                        binding,
                        invocation: invocation_context.clone(),
                        history_query,
                    }),
                }),
            }
        }
        let selected_binding = self.inspector_action_editor.selected_binding();
        if let Some(binding) = selected_binding {
            ui.separator();
            ui.small("Assign the selected result to another slot:");
            ui.horizontal_wrapped(|ui| {
                for (label, gesture) in [
                    (
                        "Cell secondary",
                        crate::radial::model::ClickGesture::Secondary,
                    ),
                    ("Cell Ctrl", crate::radial::model::ClickGesture::CtrlPrimary),
                    (
                        "Cell Shift",
                        crate::radial::model::ClickGesture::ShiftPrimary,
                    ),
                    ("Cell Alt", crate::radial::model::ClickGesture::AltPrimary),
                ] {
                    if ui.button(label).clicked() {
                        let result = assign_alternate_click(
                            session,
                            &menu_id,
                            &ring_id,
                            &cell_id,
                            gesture,
                            binding.clone(),
                        );
                        report_action_assignment_result(session, result);
                    }
                }
                for (label, target) in [
                    ("Center L", MenuActionSlot::CenterPrimary),
                    ("Center R", MenuActionSlot::CenterSecondary),
                    ("Background L", MenuActionSlot::BackgroundPrimary),
                    ("Background R", MenuActionSlot::BackgroundSecondary),
                ] {
                    if ui.button(label).clicked() {
                        let result = assign_menu_action(session, &menu_id, target, binding.clone());
                        report_action_assignment_result(session, result);
                    }
                }
            });
        }
        if let Some(operation) = deferred_operation {
            queue_scope_change(
                &mut self.pending_scope_change,
                session,
                ScopeChangeIntent::Operation(operation),
            );
        }
    }

    fn apply_post_render(&mut self) {
        let commands = std::mem::take(&mut self.post_render);
        for command in commands {
            match command {
                EditorCommand::MoveCell {
                    source_menu,
                    source_ring,
                    cell,
                    destination_menu,
                    destination_ring,
                    index,
                    expected_generation,
                } => {
                    let Some(session) = self.session.as_ref() else {
                        continue;
                    };
                    if session.generation != expected_generation {
                        self.resource_notice = Some(ResourceNotice::warning(
                            "Cell move is stale; release the drag and try again",
                        ));
                        continue;
                    }
                    let Some(destination_cell) = session
                        .draft
                        .menus
                        .iter()
                        .find(|menu| menu.id == destination_menu)
                        .and_then(|menu| menu.rings.iter().find(|ring| ring.id == destination_ring))
                        .and_then(|ring| ring.cells.get(index))
                        .map(|cell| cell.id.clone())
                    else {
                        self.resource_notice = Some(ResourceNotice::warning(
                            "Cell move destination no longer exists",
                        ));
                        continue;
                    };
                    let pending = PendingCellDrop {
                        source_menu: source_menu.clone(),
                        source_ring: source_ring.clone(),
                        source_cell: cell.clone(),
                        destination_menu: destination_menu.clone(),
                        destination_ring: destination_ring.clone(),
                        destination_cell,
                        destination_index: index,
                        generation: expected_generation,
                    };
                    let source_is_current_selection = session.selection.as_ref()
                        == Some(&StableSelection::Cell {
                            menu_id: source_menu.clone(),
                            ring_id: source_ring.clone(),
                            cell_id: cell.clone(),
                        });
                    if source_menu != destination_menu
                        || source_ring != destination_ring
                        || !source_is_current_selection
                    {
                        self.queue_editor_operation(DeferredEditorOperation::MoveCell {
                            pending,
                            resolution: menu::CellDropResolution::MoveIntoSpacer,
                        });
                        continue;
                    }
                    let result = self.session.as_mut().map_or_else(
                        || Err(menu::MenuEditError::MissingEntity),
                        |session| {
                            menu::move_cell_to_slot(
                                session,
                                (&source_menu, &source_ring, &cell),
                                (&destination_menu, &destination_ring, index),
                                menu::CellDropResolution::MoveIntoSpacer,
                                expected_generation,
                            )
                        },
                    );
                    match result {
                        Ok(()) => {
                            self.focus_restore = self
                                .session
                                .as_ref()
                                .and_then(|session| session.selection.clone());
                            acceptance_trace::emit(Event::DesignerWidget {
                                category: WidgetCategory::Canvas,
                                response: WidgetResponse::Accepted,
                                correlation: trace_correlation(self.session.as_ref()),
                            });
                        }
                        Err(menu::MenuEditError::DestinationOccupied) => {
                            acceptance_trace::emit(Event::DesignerWidget {
                                category: WidgetCategory::Canvas,
                                response: WidgetResponse::Rejected,
                                correlation: trace_correlation(self.session.as_ref()),
                            });
                            self.pending_drop = Some(pending);
                        }
                        Err(_) => acceptance_trace::emit(Event::DesignerWidget {
                            category: WidgetCategory::Canvas,
                            response: WidgetResponse::Rejected,
                            correlation: trace_correlation(self.session.as_ref()),
                        }),
                    }
                }
            }
        }
    }

    fn cancel(&mut self) {
        self.preview.cancel_tooltip();
        self.close_intent = CloseIntent::DiscardRequested;
        if let Some(session) = self.session.as_mut() {
            session.request_discard_close_intent();
        }
        self.stop_native_preview();
        let Some(session) = self.session.as_mut() else {
            self.force_close();
            return;
        };
        let result = session.request_commit(CommitDisposition::RevertAppliedAndClose);
        match result {
            Ok(request) => {
                let pending = session.pending_request;
                let result = self.client.as_ref().map_or_else(
                    || Err(AuthoringError::ServiceClosed),
                    |client| client.send(request),
                );
                if let Err(error) = result
                    && let Some(pending) = pending
                {
                    session.reconcile_request_delivery_failure(pending, format!("{error:?}"));
                    self.close_intent = CloseIntent::Requested;
                    session.request_close_intent();
                    self.close_prompt = true;
                }
            }
            Err(AuthoringError::NothingToRevert) => {
                // If StopNativePreview was queued, retain the session until
                // its terminal reply; otherwise this discard is already a
                // safe clean teardown.
                self.close_prompt = false;
                self.maybe_finish_close();
            }
            Err(error) => {
                session.last_error = Some(format!("{error:?}"));
                self.close_intent = CloseIntent::Requested;
                session.request_close_intent();
                self.close_prompt = true;
            }
        }
    }

    fn radial_add_to_menu_ui(&mut self, ctx: &egui::Context, ui_pass: &mut RadialUiPass) {
        self.radial_add_to_menu_ui_for_instance(ctx, None, ui_pass);
    }

    fn radial_add_to_menu_ui_for_instance(
        &mut self,
        ctx: &egui::Context,
        parent_instance: Option<u64>,
        ui_pass: &mut RadialUiPass,
    ) {
        let Some(request) = self.radial_add_request.clone() else {
            return;
        };
        let child_subject = request.trace_identity.request_id;
        let (window_id, native_owner) = self.radial_child_window_config(
            ui_pass,
            parent_instance,
            RadialDesignerChildKind::AddToRadial,
            child_subject,
            egui::Id::new("radial-add-to-menu"),
        );
        let original_destination = (
            self.radial_add_menu.clone(),
            self.radial_add_ring.clone(),
            self.radial_add_cell.clone(),
            self.radial_add_replace_confirmed,
            self.radial_add_close_tree_confirmed,
        );
        // Properties owns a separate staged draft. Insertion advances the
        // authoring generation, so keep its popup fully interactive and defer
        // this request until that draft is explicitly applied or cancelled.
        let properties_dirty = self.properties_popup_dirty();
        if properties_dirty {
            return;
        }
        let Some(session) = self.session.as_ref() else {
            self.radial_add_request = None;
            self.radial_add_close_tree_confirmed = false;
            return;
        };
        let snapshot_pending = session.is_initial_snapshot_pending();
        let conflict_reason = session
            .conflict
            .as_ref()
            .map(|conflict| conflict.reason.clone());
        let menus = session.draft.menus.clone();
        let default_menu_id = session.draft.default_menu_id.clone();
        let current_menu_id = selected_menu_id(session);
        let destination_before = (
            self.radial_add_menu.clone(),
            self.radial_add_ring.clone(),
            self.radial_add_cell.clone(),
        );

        if !snapshot_pending {
            if !menus
                .iter()
                .any(|menu| Some(&menu.id) == self.radial_add_menu.as_ref())
            {
                self.radial_add_menu = menus
                    .iter()
                    .find(|menu| Some(&menu.id) == current_menu_id.as_ref())
                    .or_else(|| menus.iter().find(|menu| menu.id == default_menu_id))
                    .map(|menu| menu.id.clone());
            }
            let selected_menu = self
                .radial_add_menu
                .as_ref()
                .and_then(|id| menus.iter().find(|menu| &menu.id == id));
            if !selected_menu.is_some_and(|menu| {
                menu.rings
                    .iter()
                    .any(|ring| Some(&ring.id) == self.radial_add_ring.as_ref())
            }) {
                self.radial_add_ring = selected_menu
                    .and_then(|menu| menu.rings.first())
                    .map(|ring| ring.id.clone());
            }
            let selected_ring = selected_menu.and_then(|menu| {
                self.radial_add_ring
                    .as_ref()
                    .and_then(|id| menu.rings.iter().find(|ring| &ring.id == id))
            });
            if !selected_ring.is_some_and(|ring| {
                ring.cells
                    .iter()
                    .any(|cell| Some(&cell.id) == self.radial_add_cell.as_ref())
            }) {
                self.radial_add_cell = selected_ring
                    .and_then(|ring| ring.cells.first())
                    .map(|cell| cell.id.clone());
            }
        }
        if destination_before
            != (
                self.radial_add_menu.clone(),
                self.radial_add_ring.clone(),
                self.radial_add_cell.clone(),
            )
        {
            self.radial_add_replace_confirmed = false;
            self.radial_add_close_tree_confirmed = false;
        }
        let destination_before_widgets = (
            self.radial_add_menu.clone(),
            self.radial_add_ring.clone(),
            self.radial_add_cell.clone(),
        );

        let selected_menu = self
            .radial_add_menu
            .as_ref()
            .and_then(|id| menus.iter().find(|menu| &menu.id == id));
        let selected_ring = selected_menu.and_then(|menu| {
            self.radial_add_ring
                .as_ref()
                .and_then(|id| menu.rings.iter().find(|ring| &ring.id == id))
        });
        let selected_cell = selected_ring.and_then(|ring| {
            self.radial_add_cell
                .as_ref()
                .and_then(|id| ring.cells.iter().find(|cell| &cell.id == id))
        });
        let selected_policy_preview = selected_menu.zip(selected_cell).map(|(menu, cell)| {
            menu::action_insert_after_action_policy(
                &session.draft,
                menu,
                cell.after_action,
                &request.binding,
                false,
            )
        });
        let selected_policy_needs_confirmation =
            selected_policy_preview.as_ref().is_some_and(Result::is_err);
        let selected_policy_will_close_tree = selected_cell.is_some_and(|cell| {
            cell.after_action == AfterActionPolicy::Inherit
                && selected_policy_preview
                    .as_ref()
                    .is_some_and(|preview| *preview == Ok(AfterActionPolicy::CloseTree))
        });
        let append_policy_will_close_tree = selected_menu.is_some_and(|menu| {
            menu::action_insert_after_action_policy(
                &session.draft,
                menu,
                AfterActionPolicy::Inherit,
                &request.binding,
                false,
            ) == Ok(AfterActionPolicy::CloseTree)
        });
        let document_before_digest = radial_add_document_digest(session);
        let prior_binding_digest =
            if selected_cell.is_some_and(|cell| matches!(&cell.content, CellContent::Spacer)) {
                acceptance_trace::private_trace_parts_digest(&["radial-add-empty-spacer"])
            } else {
                radial_add_cell_binding_digest(
                    session,
                    self.radial_add_menu.as_ref(),
                    self.radial_add_ring.as_ref(),
                    self.radial_add_cell.as_ref(),
                )
            };
        let prior_binding_digest = if prior_binding_digest == 0 {
            acceptance_trace::private_trace_parts_digest(&["radial-add-no-prior-binding"])
        } else {
            prior_binding_digest
        };
        let mut window_open = true;
        let mut visible_commit = false;
        let mut cancel = false;
        let mut insertion: Option<(MenuId, RingId, menu::ActionInsertDestination, bool)> = None;
        egui::Window::new("Add to radial")
            .id(window_id)
            .collapsible(false)
            .resizable(false)
            .open(&mut window_open)
            .show(ctx, |ui| {
                visible_commit = native_owner
                    && ctx.current_pass_index() == 0
                    && ui.is_visible()
                    && !ui.is_sizing_pass();
                ui_pass.commit_allowed = visible_commit;
                if !native_owner {
                    ui.disable();
                }
                ui.label(format!("Add {}", request.label));
                if !request.source_query.is_empty() {
                    ui.small(format!("Source query: {:?}", request.source_query));
                }
                if snapshot_pending {
                    ui.spinner();
                    ui.label("Waiting for the Designer's authoritative snapshot before choosing a destination.");
                } else if let Some(reason) = conflict_reason.as_deref() {
                    ui.label(format!(
                        "Resolve the Designer conflict before inserting this action: {reason}"
                    ));
                } else if self.close_intent != CloseIntent::None {
                    ui.label("Resolve the Designer close decision before inserting this action.");
                } else {
                    let selected_menu_label = selected_menu.map_or("Choose a menu", |menu| menu.name.as_str());
                    let menu_selector = ui.horizontal(|ui| {
                        let combo = egui::ComboBox::from_id_salt("radial-add-menu-selector")
                            .selected_text(selected_menu_label)
                            .show_ui(ui, |ui| {
                            for menu in &menus {
                                let response = ui.selectable_value(
                                    &mut self.radial_add_menu,
                                    Some(menu.id.clone()),
                                    &menu.name,
                                );
                                ui_pass.register_combo_option(
                                    ui,
                                    &response,
                                    "radial-add/menu",
                                    &format!("radial-add/menu/{:016x}", radial_value_digest(&menu.id)),
                                    "Destination menu",
                                );
                                let option_ring = menu.rings.first();
                                let option_cell = option_ring.and_then(|ring| ring.cells.first());
                                let option_binding = radial_add_cell_binding_digest(
                                    session,
                                    Some(&menu.id),
                                    option_ring.map(|ring| &ring.id),
                                    option_cell.map(|cell| &cell.id),
                                );
                                emit_radial_add_widget_control(
                                    ui,
                                    &response,
                                    request.trace_identity,
                                    RadialInsertionControl::DestinationMenu,
                                    acceptance_trace::RadialInsertionWidgetPart::Option,
                                    Some(session),
                                    Some(&menu.id),
                                    option_ring.map(|ring| &ring.id),
                                    option_cell.map(|cell| &cell.id),
                                    response.enabled(),
                                    self.radial_add_menu.as_ref() == Some(&menu.id),
                                    response.clicked(),
                                    document_before_digest,
                                    option_binding,
                                );
                                if response.clicked() {
                                    emit_radial_add_control(
                                        request.trace_identity,
                                        RadialInsertionControl::DestinationMenu,
                                        Some(session),
                                        self.radial_add_menu.as_ref(),
                                        self.radial_add_ring.as_ref(),
                                        self.radial_add_cell.as_ref(),
                                        response.enabled(),
                                        self.radial_add_menu.as_ref() == Some(&menu.id),
                                        true,
                                        document_before_digest,
                                        prior_binding_digest,
                                    );
                                }
                            }
                        });
                        ui_pass.register_combo(ui, &combo.response, "radial-add/menu", "Destination menu");
                        ui.label("Menu");
                        combo.response
                    }).inner;
                    emit_radial_add_widget_control(
                        ui,
                        &menu_selector,
                        request.trace_identity,
                        RadialInsertionControl::DestinationMenu,
                        acceptance_trace::RadialInsertionWidgetPart::Selector,
                        Some(session),
                        self.radial_add_menu.as_ref(),
                        self.radial_add_ring.as_ref(),
                        self.radial_add_cell.as_ref(),
                        menu_selector.enabled(),
                        selected_menu.is_some(),
                        menu_selector.clicked(),
                        document_before_digest,
                        prior_binding_digest,
                    );
                    let ring_label = selected_ring.map_or("Choose a ring", |ring| {
                        ring.id.as_str()
                    });
                    let ring_selector = ui.horizontal(|ui| {
                        let combo = egui::ComboBox::from_id_salt("radial-add-ring-selector")
                            .selected_text(ring_label)
                            .show_ui(ui, |ui| {
                            if let Some(menu) = selected_menu {
                                for ring in &menu.rings {
                                    let response = ui.selectable_value(
                                        &mut self.radial_add_ring,
                                        Some(ring.id.clone()),
                                        format!("{} · {} slots", ring.id, ring.cells.len()),
                                    );
                                    ui_pass.register_combo_option(
                                        ui,
                                        &response,
                                        "radial-add/ring",
                                        &format!("radial-add/ring/{:016x}", radial_value_digest(&(&menu.id, &ring.id))),
                                        "Destination ring",
                                    );
                                    let option_cell = ring
                                        .cells
                                        .iter()
                                        .find(|cell| Some(&cell.id) == self.radial_add_cell.as_ref())
                                        .or_else(|| ring.cells.first());
                                    let option_binding = radial_add_cell_binding_digest(
                                        session,
                                        Some(&menu.id),
                                        Some(&ring.id),
                                        option_cell.map(|cell| &cell.id),
                                    );
                                    emit_radial_add_widget_control(
                                        ui,
                                        &response,
                                        request.trace_identity,
                                        RadialInsertionControl::DestinationRing,
                                        acceptance_trace::RadialInsertionWidgetPart::Option,
                                        Some(session),
                                        Some(&menu.id),
                                        Some(&ring.id),
                                        option_cell.map(|cell| &cell.id),
                                        response.enabled(),
                                        self.radial_add_ring.as_ref() == Some(&ring.id),
                                        response.clicked(),
                                        document_before_digest,
                                        option_binding,
                                    );
                                    if response.clicked() {
                                        emit_radial_add_control(
                                            request.trace_identity,
                                            RadialInsertionControl::DestinationRing,
                                            Some(session),
                                            self.radial_add_menu.as_ref(),
                                            self.radial_add_ring.as_ref(),
                                            self.radial_add_cell.as_ref(),
                                            response.enabled(),
                                            self.radial_add_ring.as_ref() == Some(&ring.id),
                                            true,
                                            document_before_digest,
                                            prior_binding_digest,
                                        );
                                    }
                                }
                            }
                        });
                        ui_pass.register_combo(ui, &combo.response, "radial-add/ring", "Destination ring");
                        ui.label("Ring");
                        combo.response
                    }).inner;
                    emit_radial_add_widget_control(
                        ui,
                        &ring_selector,
                        request.trace_identity,
                        RadialInsertionControl::DestinationRing,
                        acceptance_trace::RadialInsertionWidgetPart::Selector,
                        Some(session),
                        self.radial_add_menu.as_ref(),
                        self.radial_add_ring.as_ref(),
                        self.radial_add_cell.as_ref(),
                        ring_selector.enabled(),
                        selected_ring.is_some(),
                        ring_selector.clicked(),
                        document_before_digest,
                        prior_binding_digest,
                    );
                    let cell_label = selected_cell.map_or_else(
                        || "Choose a cell".to_owned(),
                        |cell| format!("{} · {}", cell.label, cell_content_kind(&cell.content)),
                    );
                    let cell_selector = ui.horizontal(|ui| {
                        let combo = egui::ComboBox::from_id_salt("radial-add-cell-selector")
                            .selected_text(cell_label)
                            .show_ui(ui, |ui| {
                            if let Some(ring) = selected_ring {
                                for cell in &ring.cells {
                                    let response = ui.selectable_value(
                                        &mut self.radial_add_cell,
                                        Some(cell.id.clone()),
                                        format!(
                                            "{} · {}",
                                            cell.label,
                                            cell_content_kind(&cell.content)
                                        ),
                                    );
                                    ui_pass.register_combo_option(
                                        ui,
                                        &response,
                                        "radial-add/cell",
                                        &format!("radial-add/cell/{:016x}", radial_value_digest(&(&cell.id, radial_add_cell_binding_digest(session, self.radial_add_menu.as_ref(), self.radial_add_ring.as_ref(), Some(&cell.id))))),
                                        "Destination cell",
                                    );
                                    let option_binding = radial_add_cell_binding_digest(
                                        session,
                                        self.radial_add_menu.as_ref(),
                                        self.radial_add_ring.as_ref(),
                                        Some(&cell.id),
                                    );
                                    emit_radial_add_widget_control(
                                        ui,
                                        &response,
                                        request.trace_identity,
                                        RadialInsertionControl::DestinationCell,
                                        acceptance_trace::RadialInsertionWidgetPart::Option,
                                        Some(session),
                                        self.radial_add_menu.as_ref(),
                                        self.radial_add_ring.as_ref(),
                                        Some(&cell.id),
                                        response.enabled(),
                                        self.radial_add_cell.as_ref() == Some(&cell.id),
                                        response.clicked(),
                                        document_before_digest,
                                        option_binding,
                                    );
                                    if response.clicked() {
                                        let selected_cell_binding_digest =
                                            radial_add_cell_binding_digest(
                                                session,
                                                self.radial_add_menu.as_ref(),
                                                self.radial_add_ring.as_ref(),
                                                self.radial_add_cell.as_ref(),
                                            );
                                        emit_radial_add_control(
                                            request.trace_identity,
                                            RadialInsertionControl::DestinationCell,
                                            Some(session),
                                            self.radial_add_menu.as_ref(),
                                            self.radial_add_ring.as_ref(),
                                            self.radial_add_cell.as_ref(),
                                            response.enabled(),
                                            self.radial_add_cell.as_ref() == Some(&cell.id),
                                            true,
                                            document_before_digest,
                                            selected_cell_binding_digest,
                                        );
                                    }
                                }
                            }
                        });
                        ui_pass.register_combo(ui, &combo.response, "radial-add/cell", "Destination cell");
                        ui.label("Cell");
                        combo.response
                    }).inner;
                    emit_radial_add_widget_control(
                        ui,
                        &cell_selector,
                        request.trace_identity,
                        RadialInsertionControl::DestinationCell,
                        acceptance_trace::RadialInsertionWidgetPart::Selector,
                        Some(session),
                        self.radial_add_menu.as_ref(),
                        self.radial_add_ring.as_ref(),
                        self.radial_add_cell.as_ref(),
                        cell_selector.enabled(),
                        selected_cell.is_some(),
                        cell_selector.clicked(),
                        document_before_digest,
                        prior_binding_digest,
                    );
                    if selected_policy_will_close_tree {
                        ui.small(
                            "Adding here will set this action to CloseTree so it can hand off to the launcher.",
                        );
                    } else if selected_policy_needs_confirmation {
                        ui.small(
                            "This cell explicitly uses KeepOpen, which is incompatible with this action's handoff.",
                        );
                        let response = ui.checkbox(
                            &mut self.radial_add_close_tree_confirmed,
                            "Confirm CloseTree for the inserted action",
                        );
                        ui_pass.register(ui, &response, "radial-add/close-tree", "Confirm action behavior", TargetKind::Button);
                        emit_radial_add_widget_control(
                            ui,
                            &response,
                            request.trace_identity,
                            RadialInsertionControl::CloseTreeConfirm,
                            acceptance_trace::RadialInsertionWidgetPart::Button,
                            Some(session),
                            self.radial_add_menu.as_ref(),
                            self.radial_add_ring.as_ref(),
                            self.radial_add_cell.as_ref(),
                            response.enabled(),
                            self.radial_add_close_tree_confirmed,
                            response.clicked(),
                            document_before_digest,
                            prior_binding_digest,
                        );
                        if response.changed() || response.clicked() {
                            emit_radial_add_control(
                                request.trace_identity,
                                RadialInsertionControl::CloseTreeConfirm,
                                Some(session),
                                self.radial_add_menu.as_ref(),
                                self.radial_add_ring.as_ref(),
                                self.radial_add_cell.as_ref(),
                                response.enabled(),
                                self.radial_add_close_tree_confirmed,
                                response.clicked(),
                                document_before_digest,
                                prior_binding_digest,
                            );
                        }
                    }
                    if append_policy_will_close_tree {
                        ui.small(
                            "An appended cell will use CloseTree so the action can hand off to the launcher.",
                        );
                    }
                    ui.horizontal(|ui| {
                        if let (Some(menu), Some(ring), Some(cell)) =
                            (selected_menu, selected_ring, selected_cell)
                        {
                            if matches!(&cell.content, CellContent::Spacer) {
                                let response = ui
                                    .add_enabled(
                                        !selected_policy_needs_confirmation
                                            || self.radial_add_close_tree_confirmed,
                                        egui::Button::new("Add to selected spacer"),
                                    );
                                ui_pass.register(ui, &response, "radial-add/insert-selected", "Add to selected cell", TargetKind::Button);
                                emit_radial_add_widget_control(
                                    ui,
                                    &response,
                                    request.trace_identity,
                                    RadialInsertionControl::InsertSelectedSpacer,
                                    acceptance_trace::RadialInsertionWidgetPart::Button,
                                    Some(session),
                                    Some(&menu.id),
                                    Some(&ring.id),
                                    Some(&cell.id),
                                    response.enabled(),
                                    true,
                                    response.clicked(),
                                    document_before_digest,
                                    prior_binding_digest,
                                );
                                if response.clicked() {
                                    insertion = Some((
                                        menu.id.clone(),
                                        ring.id.clone(),
                                        menu::ActionInsertDestination::Cell(cell.id.clone()),
                                        false,
                                    ));
                                }
                            } else {
                                let response = ui.checkbox(
                                    &mut self.radial_add_replace_confirmed,
                                    "Replace occupied cell",
                                );
                                ui_pass.register(ui, &response, "radial-add/replace-toggle", "Confirm replacement", TargetKind::Button);
                                emit_radial_add_widget_control(
                                    ui,
                                    &response,
                                    request.trace_identity,
                                    RadialInsertionControl::ReplaceToggle,
                                    acceptance_trace::RadialInsertionWidgetPart::Button,
                                    Some(session),
                                    Some(&menu.id),
                                    Some(&ring.id),
                                    Some(&cell.id),
                                    response.enabled(),
                                    self.radial_add_replace_confirmed,
                                    response.clicked(),
                                    document_before_digest,
                                    prior_binding_digest,
                                );
                                if !self.radial_add_replace_confirmed || response.changed() {
                                    emit_radial_add_control(
                                        request.trace_identity,
                                        RadialInsertionControl::ReplaceToggle,
                                        Some(session),
                                        self.radial_add_menu.as_ref(),
                                        self.radial_add_ring.as_ref(),
                                        self.radial_add_cell.as_ref(),
                                        response.enabled(),
                                        self.radial_add_replace_confirmed,
                                        response.clicked(),
                                        document_before_digest,
                                        prior_binding_digest,
                                    );
                                }
                                let response = ui
                                    .add_enabled(
                                        self.radial_add_replace_confirmed
                                            && (!selected_policy_needs_confirmation
                                                || self.radial_add_close_tree_confirmed),
                                        egui::Button::new("Replace selected cell"),
                                    );
                                ui_pass.register(ui, &response, "radial-add/replace", "Replace selected cell", TargetKind::Button);
                                emit_radial_add_widget_control(
                                    ui,
                                    &response,
                                    request.trace_identity,
                                    RadialInsertionControl::ReplaceConfirm,
                                    acceptance_trace::RadialInsertionWidgetPart::Button,
                                    Some(session),
                                    Some(&menu.id),
                                    Some(&ring.id),
                                    Some(&cell.id),
                                    response.enabled(),
                                    response.enabled(),
                                    response.clicked(),
                                    document_before_digest,
                                    prior_binding_digest,
                                );
                                if !response.enabled() && !response.clicked() {
                                    emit_radial_add_control(
                                        request.trace_identity,
                                        RadialInsertionControl::ReplaceConfirm,
                                        Some(session),
                                        self.radial_add_menu.as_ref(),
                                        self.radial_add_ring.as_ref(),
                                        self.radial_add_cell.as_ref(),
                                        false,
                                        false,
                                        false,
                                        document_before_digest,
                                        prior_binding_digest,
                                    );
                                }
                                if response.clicked() {
                                    insertion = Some((
                                        menu.id.clone(),
                                        ring.id.clone(),
                                        menu::ActionInsertDestination::Cell(cell.id.clone()),
                                        true,
                                    ));
                                }
                            }
                        }
                        if let (Some(menu), Some(ring)) = (selected_menu, selected_ring) {
                            let can_append = ring.cells.len()
                                < crate::radial::model::limits::MAX_CELLS_PER_RING;
                            let response = ui
                                .add_enabled(can_append, egui::Button::new("Append to ring"))
                                ;
                            ui_pass.register(ui, &response, "radial-add/append", "Append to ring", TargetKind::Button);
                            emit_radial_add_widget_control(
                                ui,
                                &response,
                                request.trace_identity,
                                RadialInsertionControl::AppendToRing,
                                acceptance_trace::RadialInsertionWidgetPart::Button,
                                Some(session),
                                Some(&menu.id),
                                Some(&ring.id),
                                self.radial_add_cell.as_ref(),
                                response.enabled(),
                                can_append,
                                response.clicked(),
                                document_before_digest,
                                prior_binding_digest,
                            );
                                if response.clicked() {
                                insertion = Some((
                                    menu.id.clone(),
                                    ring.id.clone(),
                                    menu::ActionInsertDestination::Append,
                                    false,
                                ));
                            }
                        }
                    });
                }
                ui.separator();
                let response = ui.button("Cancel Add");
                ui_pass.register(ui, &response, "radial-add/cancel", "Cancel add", TargetKind::Button);
                emit_radial_add_widget_control(
                    ui,
                    &response,
                    request.trace_identity,
                    RadialInsertionControl::Cancel,
                    acceptance_trace::RadialInsertionWidgetPart::Button,
                    Some(session),
                    self.radial_add_menu.as_ref(),
                    self.radial_add_ring.as_ref(),
                    self.radial_add_cell.as_ref(),
                    response.enabled(),
                    false,
                    response.clicked(),
                    document_before_digest,
                    prior_binding_digest,
                );
                if response.clicked() {
                    emit_radial_add_control(
                        request.trace_identity,
                        RadialInsertionControl::Cancel,
                        Some(session),
                        self.radial_add_menu.as_ref(),
                        self.radial_add_ring.as_ref(),
                        self.radial_add_cell.as_ref(),
                        response.enabled(),
                        false,
                        true,
                        document_before_digest,
                        prior_binding_digest,
                    );
                    cancel = true;
                }
            });

        let titlebar_close = native_owner && ctx.current_pass_index() == 0 && !window_open;
        if !native_owner || (!visible_commit && !titlebar_close) {
            (
                self.radial_add_menu,
                self.radial_add_ring,
                self.radial_add_cell,
                self.radial_add_replace_confirmed,
                self.radial_add_close_tree_confirmed,
            ) = original_destination;
            return;
        }
        ui_pass.commit_allowed = visible_commit;
        if titlebar_close {
            window_open = false;
        }

        let destination_after_widgets = (
            self.radial_add_menu.clone(),
            self.radial_add_ring.clone(),
            self.radial_add_cell.clone(),
        );
        if destination_before_widgets != destination_after_widgets {
            self.radial_add_replace_confirmed = false;
            self.radial_add_close_tree_confirmed = false;
            insertion = None;
        }

        if !window_open || cancel {
            self.radial_add_request = None;
            self.radial_add_replace_confirmed = false;
            self.radial_add_close_tree_confirmed = false;
            return;
        }
        let Some((menu_id, ring_id, destination, replace_occupied)) = insertion else {
            return;
        };
        let trace_control = match (&destination, replace_occupied) {
            (menu::ActionInsertDestination::Append, _) => RadialInsertionControl::AppendToRing,
            (menu::ActionInsertDestination::Cell(_), true) => {
                RadialInsertionControl::ReplaceConfirm
            }
            (menu::ActionInsertDestination::Cell(_), false) => {
                RadialInsertionControl::InsertSelectedSpacer
            }
        };
        let result = self.commit_pending_radial_add(
            &menu_id,
            &ring_id,
            destination,
            replace_occupied,
            self.radial_add_close_tree_confirmed,
        );
        match result {
            Ok(cell_id) => {
                let committed = self.session.as_ref();
                let document_after_digest = committed.map_or(0, radial_add_document_digest);
                let binding_after_digest = committed.map_or(0, |session| {
                    radial_add_cell_binding_digest(
                        session,
                        Some(&menu_id),
                        Some(&ring_id),
                        Some(&cell_id),
                    )
                });
                if let Some(request) = self.radial_add_request.as_ref() {
                    emit_radial_add_control(
                        request.trace_identity,
                        trace_control,
                        committed,
                        Some(&menu_id),
                        Some(&ring_id),
                        Some(&cell_id),
                        true,
                        true,
                        true,
                        document_after_digest,
                        binding_after_digest,
                    );
                    self.last_radial_add_trace = Some(RadialInsertionTraceContext {
                        identity: request.trace_identity,
                        menu_id: menu_id.clone(),
                        ring_id: ring_id.clone(),
                        cell_id: cell_id.clone(),
                        prior_binding_digest,
                    });
                }
                self.radial_add_request = None;
                self.radial_add_replace_confirmed = false;
                self.radial_add_close_tree_confirmed = false;
                self.resource_notice = Some(ResourceNotice::info(
                    "Action added to the draft. Save remains an explicit Designer action.",
                ));
            }
            Err(error) => {
                self.resource_notice = Some(ResourceNotice::error(format!(
                    "Could not add action to radial: {error:?}"
                )));
            }
        }
    }

    fn commit_pending_radial_add(
        &mut self,
        menu_id: &MenuId,
        ring_id: &RingId,
        destination: menu::ActionInsertDestination,
        replace_occupied: bool,
        close_tree_confirmed: bool,
    ) -> Result<CellId, menu::MenuEditError> {
        let properties_dirty = self.properties_popup_dirty();
        let Some(request) = self.radial_add_request.clone() else {
            return Err(menu::MenuEditError::StaleGeneration);
        };
        let Some(session) = self.session.as_mut() else {
            return Err(menu::MenuEditError::MissingEntity);
        };
        if session.is_initial_snapshot_pending()
            || session.close_requested()
            || session.conflict.is_some()
            || properties_dirty
        {
            return Err(menu::MenuEditError::StaleGeneration);
        }
        menu::add_action_binding(
            session,
            menu_id,
            ring_id,
            destination,
            request.binding,
            &request.label,
            replace_occupied,
            close_tree_confirmed,
        )
    }

    fn prompts(
        &mut self,
        ctx: &egui::Context,
        parent_instance: Option<u64>,
        ui_pass: &mut RadialUiPass,
    ) {
        if self.close_prompt {
            let correlation = trace_correlation(self.session.as_ref());
            let (window_id, native_owner) = self.radial_child_window_config(
                ui_pass,
                parent_instance,
                RadialDesignerChildKind::ClosePrompt,
                0,
                egui::Id::new("radial-designer-close-prompt"),
            );
            let mut visible_commit = false;
            egui::Window::new("Unsaved radial changes")
                .id(window_id)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    visible_commit = native_owner
                        && ctx.current_pass_index() == 0
                        && ui.is_visible()
                        && !ui.is_sizing_pass();
                    ui_pass.commit_allowed = visible_commit;
                    if !native_owner {
                        ui.disable();
                    }
                    let popup_dirty = self.properties_popup_dirty();
                    ui.label(if popup_dirty {
                        "Cell properties still have unapplied edits. Keep editing to Apply them, or discard all changes."
                    } else {
                        "Save, discard, or keep editing?"
                    });
                    ui.horizontal(|ui| {
                        let save = ui
                            .add_enabled(!popup_dirty, egui::Button::new("Save"))
                            .on_disabled_hover_text("Apply or cancel the open cell properties first");
                        let save = with_button_emphasis(ui, save, ButtonEmphasis::Primary);
                        ui_pass.register(ui, &save, "radial-close/save", "Save changes", TargetKind::Button);
                        if save.clicked() && visible_commit {
                            self.close_prompt = false;
                            self.send_commit(CommitDisposition::Save);
                        }
                        let button = ui.button("Discard");
                        let response =
                            with_button_emphasis(ui, button, ButtonEmphasis::Destructive);
                        ui_pass.register(ui, &response, "radial-close/discard", "Discard changes", TargetKind::Button);
                        trace_designer_authoring_control(
                            ui,
                            &response,
                            DesignerAuthoringTarget::DiscardDraft,
                            DesignerAuthoringRole::Button,
                            None,
                            true,
                            false,
                            ViewportClass::Deferred,
                            correlation,
                        );
                        if response.clicked() && visible_commit {
                            self.close_intent = CloseIntent::DiscardRequested;
                            if let Some(session) = self.session.as_mut() {
                                session.request_discard_close_intent();
                            }
                            self.close_prompt = false;
                            self.cancel();
                        }
                        let response = ui.button("Keep editing");
                        ui_pass.register(ui, &response, "radial-close/keep", "Keep editing", TargetKind::Button);
                        trace_designer_authoring_control(
                            ui,
                            &response,
                            DesignerAuthoringTarget::KeepEditing,
                            DesignerAuthoringRole::Button,
                            None,
                            true,
                            false,
                            ViewportClass::Deferred,
                            correlation,
                        );
                        if response.clicked() && visible_commit {
                            self.close_prompt = false;
                            self.close_intent = CloseIntent::None;
                            self.close_stop_attempted = false;
                            if let Some(session) = self.session.as_mut() {
                                session.clear_close_intent();
                            }
                            self.trace_close_state();
                        }
                    });
                });
        }
        if let Some(message) = self.delete_message.clone() {
            let (window_id, native_owner) = self.radial_child_window_config(
                ui_pass,
                parent_instance,
                RadialDesignerChildKind::DeleteMessage,
                0,
                egui::Id::new("radial-designer-delete-message"),
            );
            let mut visible_commit = false;
            egui::Window::new("Delete impact")
                .id(window_id)
                .show(ctx, |ui| {
                    visible_commit = native_owner
                        && ctx.current_pass_index() == 0
                        && ui.is_visible()
                        && !ui.is_sizing_pass();
                    ui_pass.commit_allowed = visible_commit;
                    if !native_owner {
                        ui.disable();
                    }
                    ui.label(message);
                    let response = ui.button("OK");
                    ui_pass.register(
                        ui,
                        &response,
                        "radial-delete-message/ok",
                        "Dismiss message",
                        TargetKind::Button,
                    );
                    if response.clicked() && visible_commit {
                        self.delete_message = None;
                    }
                });
        }
        if let Some((menu_id, ring_id)) = self.delete_ring_prompt.clone() {
            let destinations: Vec<_> = self
                .session
                .as_ref()
                .into_iter()
                .flat_map(|session| session.draft.menus.iter())
                .filter(|menu| menu.id == menu_id)
                .flat_map(|menu| menu.rings.iter())
                .filter(|ring| ring.id != ring_id)
                .map(|ring| ring.id.clone())
                .collect();
            let populated = self
                .session
                .as_ref()
                .and_then(|session| session.draft.menus.iter().find(|menu| menu.id == menu_id))
                .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
                .map_or(0, |ring| {
                    ring.cells
                        .iter()
                        .filter(|cell| !matches!(cell.content, CellContent::Spacer))
                        .count()
                });
            let subject = radial_value_digest(&(&menu_id, &ring_id));
            let (window_id, native_owner) = self.radial_child_window_config(
                ui_pass,
                parent_instance,
                RadialDesignerChildKind::DeleteRing,
                subject,
                egui::Id::new("radial-designer-delete-ring"),
            );
            let mut visible_commit = false;
            egui::Window::new("Delete populated ring")
                .id(window_id)
                .show(ctx, |ui| {
                    visible_commit = native_owner
                        && ctx.current_pass_index() == 0
                        && ui.is_visible()
                        && !ui.is_sizing_pass();
                    ui_pass.commit_allowed = visible_commit;
                    if !native_owner {
                        ui.disable();
                    }
                    ui.label(format!(
                    "This removes {populated} populated cells. Relocate them or explicitly discard."
                ));
                    for destination in destinations {
                        let response = ui.button(format!("Move cells to {destination} and delete"));
                        let identity = format!(
                            "radial-delete-ring/move/{:016x}",
                            radial_value_digest(&destination)
                        );
                        ui_pass.register(
                            ui,
                            &response,
                            &identity,
                            "Move cells and delete",
                            TargetKind::Button,
                        );
                        if response.clicked()
                            && visible_commit
                            && let Some(session) = self.session.as_mut()
                        {
                            match relocate_and_delete_ring(
                                session,
                                &menu_id,
                                &ring_id,
                                &destination,
                            ) {
                                Ok(()) => self.delete_ring_prompt = None,
                                Err(error) => self.delete_message = Some(error),
                            }
                        }
                    }
                    let discard = ui.button("Discard populated cells and delete");
                    let discard = with_button_emphasis(ui, discard, ButtonEmphasis::Destructive);
                    ui_pass.register(
                        ui,
                        &discard,
                        "radial-delete-ring/discard",
                        "Discard cells and delete",
                        TargetKind::Button,
                    );
                    if discard.clicked()
                        && visible_commit
                        && let Some(session) = self.session.as_mut()
                    {
                        match menu::delete_ring(session, &menu_id, &ring_id) {
                            Ok(()) => self.delete_ring_prompt = None,
                            Err(error) => {
                                self.delete_message = Some(format!("Delete failed: {error:?}"))
                            }
                        }
                    }
                    let response = ui.button("Cancel");
                    ui_pass.register(
                        ui,
                        &response,
                        "radial-delete-ring/cancel",
                        "Cancel delete",
                        TargetKind::Button,
                    );
                    if response.clicked() && visible_commit {
                        self.delete_ring_prompt = None;
                    }
                });
        }
        if let Some(plan) = self.resize_prompt.clone() {
            let correlation = trace_correlation(self.session.as_ref());
            let subject = radial_value_digest(&(&plan.menu_id, &plan.ring_id, plan.requested_len));
            let (window_id, native_owner) = self.radial_child_window_config(
                ui_pass,
                parent_instance,
                RadialDesignerChildKind::ResizeResolution,
                subject,
                egui::Id::new("radial-designer-resize-resolution"),
            );
            let mut visible_commit = false;
            egui::Window::new("Resolve populated cells")
                .id(window_id)
                .show(ctx, |ui| {
                    visible_commit = native_owner
                        && ctx.current_pass_index() == 0
                        && ui.is_visible()
                        && !ui.is_sizing_pass();
                    ui_pass.commit_allowed = visible_commit;
                    if !native_owner {
                        ui.disable();
                    }
                    ui.label(format!(
                        "{} populated cells would be removed.",
                        plan.populated_removed.len()
                    ));
                    let response = ui.button("Move to overflow ring");
                    ui_pass.register(
                        ui,
                        &response,
                        "radial-resize/overflow",
                        "Move to overflow ring",
                        TargetKind::Button,
                    );
                    trace_designer_authoring_control(
                        ui,
                        &response,
                        DesignerAuthoringTarget::MoveToOverflow,
                        DesignerAuthoringRole::Button,
                        None,
                        true,
                        false,
                        ViewportClass::Deferred,
                        correlation,
                    );
                    if response.clicked() && visible_commit {
                        let proposal = self.session.as_mut().map(|session| {
                            let overflow_id = session.allocate_ring_id("overflow");
                            menu::propose_resolved_resize(
                                &session.draft,
                                session.generation,
                                &plan,
                                ResizeResolution::OverflowRing,
                                Some(overflow_id),
                            )
                        });
                        match proposal {
                            Some(Ok(proposal)) => {
                                self.resize_prompt = None;
                                self.install_ring_proposal(proposal);
                            }
                            Some(Err(error)) => {
                                self.delete_message = Some(format!("Resize failed: {error:?}"))
                            }
                            None => {}
                        }
                    }
                    let destinations: Vec<_> = self
                        .session
                        .as_ref()
                        .into_iter()
                        .flat_map(|session| session.draft.menus.iter())
                        .filter(|menu| menu.id == plan.menu_id)
                        .flat_map(|menu| menu.rings.iter())
                        .filter(|ring| ring.id != plan.ring_id)
                        .map(|ring| ring.id.clone())
                        .collect();
                    for destination in destinations {
                        let response = ui.button(format!("Relocate to {destination}"));
                        let identity = format!(
                            "radial-resize/relocate/{:016x}",
                            radial_value_digest(&destination)
                        );
                        ui_pass.register(
                            ui,
                            &response,
                            &identity,
                            "Relocate cells",
                            TargetKind::Button,
                        );
                        if response.clicked() && visible_commit {
                            let proposal = self.session.as_ref().map(|session| {
                                menu::propose_resolved_resize(
                                    &session.draft,
                                    session.generation,
                                    &plan,
                                    ResizeResolution::Relocate {
                                        menu_id: plan.menu_id.clone(),
                                        ring_id: destination,
                                    },
                                    None,
                                )
                            });
                            match proposal {
                                Some(Ok(proposal)) => {
                                    self.resize_prompt = None;
                                    self.install_ring_proposal(proposal);
                                }
                                Some(Err(error)) => {
                                    self.delete_message = Some(format!("Resize failed: {error:?}"))
                                }
                                None => {}
                            }
                        }
                    }
                    let button = ui.button("Discard cells");
                    let response = with_button_emphasis(ui, button, ButtonEmphasis::Destructive);
                    ui_pass.register(
                        ui,
                        &response,
                        "radial-resize/discard",
                        "Discard cells",
                        TargetKind::Button,
                    );
                    trace_designer_authoring_control(
                        ui,
                        &response,
                        DesignerAuthoringTarget::DiscardCells,
                        DesignerAuthoringRole::Button,
                        None,
                        true,
                        false,
                        ViewportClass::Deferred,
                        correlation,
                    );
                    if response.clicked() && visible_commit {
                        let proposal = self.session.as_ref().map(|session| {
                            menu::propose_resolved_resize(
                                &session.draft,
                                session.generation,
                                &plan,
                                ResizeResolution::ConfirmDiscard,
                                None,
                            )
                        });
                        match proposal {
                            Some(Ok(proposal)) => {
                                self.resize_prompt = None;
                                self.install_ring_proposal(proposal);
                            }
                            Some(Err(error)) => {
                                self.delete_message = Some(format!("Resize failed: {error:?}"))
                            }
                            None => {}
                        }
                    }
                    let response = ui.button("Cancel");
                    ui_pass.register(
                        ui,
                        &response,
                        "radial-resize/cancel",
                        "Cancel resize",
                        TargetKind::Button,
                    );
                    trace_designer_authoring_control(
                        ui,
                        &response,
                        DesignerAuthoringTarget::CancelResolution,
                        DesignerAuthoringRole::Button,
                        None,
                        true,
                        false,
                        ViewportClass::Deferred,
                        correlation,
                    );
                    if response.clicked() && visible_commit {
                        self.resize_prompt = None;
                    }
                });
        }
    }
}

fn cell_tree_accessible_name(cell: &crate::radial::model::CellDefinition, index: usize) -> String {
    if !cell.label.trim().is_empty() {
        return cell.label.trim().to_owned();
    }
    if let crate::radial::model::Override::Value(tooltip) = &cell.tooltip
        && !tooltip.trim().is_empty()
    {
        return tooltip.trim().to_owned();
    }
    if matches!(cell.icon, crate::radial::model::Override::Value(_)) {
        let role = match &cell.content {
            CellContent::Action { .. } => "Action",
            CellContent::Dynamic { .. } => "Dynamic item",
            CellContent::Submenu { .. } => "Submenu",
            CellContent::Control { control } => match control {
                Control::Back => "Back",
                Control::Close => "Close",
                Control::NextPage => "Next page",
                Control::PreviousPage => "Previous page",
                Control::Drag => "Drag",
            },
            CellContent::Spacer => "Spacer",
        };
        return format!("{role} icon");
    }
    format!("Cell {} ({})", index + 1, cell.id)
}

fn cell_content_kind(content: &CellContent) -> &'static str {
    match content {
        CellContent::Action { .. } => "Action",
        CellContent::Dynamic { .. } => "Dynamic item",
        CellContent::Submenu { .. } => "Submenu",
        CellContent::Control { .. } => "Control",
        CellContent::Spacer => "Spacer",
    }
}

fn primary_content_replaced(before: &CellContent, after: &CellContent) -> bool {
    match (before, after) {
        (CellContent::Spacer, CellContent::Spacer)
        | (CellContent::Dynamic { .. }, CellContent::Dynamic { .. }) => false,
        (CellContent::Submenu { menu_id: before }, CellContent::Submenu { menu_id: after }) => {
            before != after
        }
        (CellContent::Control { control: before }, CellContent::Control { control: after }) => {
            before != after
        }
        (CellContent::Action { binding: before }, CellContent::Action { binding: after }) => {
            before != after
        }
        _ => true,
    }
}

fn radial_add_destination_digest<T: std::fmt::Display>(id: Option<&T>) -> u64 {
    id.map(|id| acceptance_trace::private_trace_text_digest(&id.to_string()))
        .unwrap_or(0)
}

fn radial_add_document_digest(session: &RadialAuthoringSession) -> u64 {
    serde_json::to_vec(session.draft.as_ref())
        .ok()
        .map_or(0, |bytes| acceptance_observation_digest(&bytes))
}

fn radial_add_cell_binding_digest(
    session: &RadialAuthoringSession,
    menu_id: Option<&MenuId>,
    ring_id: Option<&RingId>,
    cell_id: Option<&CellId>,
) -> u64 {
    let cell = menu_id
        .and_then(|menu_id| session.draft.menus.iter().find(|menu| &menu.id == menu_id))
        .and_then(|menu| {
            ring_id.and_then(|ring_id| menu.rings.iter().find(|ring| &ring.id == ring_id))
        })
        .and_then(|ring| {
            cell_id.and_then(|cell_id| ring.cells.iter().find(|cell| &cell.id == cell_id))
        });
    cell.map_or_else(
        || acceptance_trace::private_trace_parts_digest(&["radial-add-no-prior-binding"]),
        |cell| match &cell.content {
            CellContent::Action { binding } => action_editor::trace_binding_digest(Some(binding)),
            CellContent::Spacer => {
                acceptance_trace::private_trace_parts_digest(&["radial-add-empty-spacer"])
            }
            content => acceptance_trace::private_trace_serialized_digest(content),
        },
    )
}

fn emit_radial_add_control(
    identity: RadialInsertionTraceIdentity,
    control: RadialInsertionControl,
    session: Option<&RadialAuthoringSession>,
    menu_id: Option<&MenuId>,
    ring_id: Option<&RingId>,
    cell_id: Option<&CellId>,
    enabled: bool,
    selected: bool,
    clicked: bool,
    document_digest_after: u64,
    binding_digest_after: u64,
) {
    acceptance_trace::emit_radial_insertion_control(
        identity,
        control,
        radial_add_destination_digest(menu_id),
        radial_add_destination_digest(ring_id),
        radial_add_destination_digest(cell_id),
        session.map_or(0, |session| session.editor_session.0),
        session.map_or(0, |session| session.generation.0),
        enabled,
        selected,
        clicked,
        document_digest_after,
        binding_digest_after,
    );
}

fn emit_radial_add_widget_control(
    ui: &egui::Ui,
    response: &egui::Response,
    identity: RadialInsertionTraceIdentity,
    control: RadialInsertionControl,
    widget_part: acceptance_trace::RadialInsertionWidgetPart,
    session: Option<&RadialAuthoringSession>,
    menu_id: Option<&MenuId>,
    ring_id: Option<&RingId>,
    cell_id: Option<&CellId>,
    enabled: bool,
    selected: bool,
    clicked: bool,
    document_digest_after: u64,
    binding_digest_after: u64,
) {
    let Some(widget) = radial_add_widget_geometry(ui, response) else {
        return;
    };
    acceptance_trace::emit_radial_insertion_control_with_widget(
        identity,
        control,
        widget_part,
        widget,
        radial_add_destination_digest(menu_id),
        radial_add_destination_digest(ring_id),
        radial_add_destination_digest(cell_id),
        session.map_or(0, |session| session.editor_session.0),
        session.map_or(0, |session| session.generation.0),
        enabled,
        selected,
        clicked,
        document_digest_after,
        binding_digest_after,
    );
}

fn radial_add_widget_geometry(
    ui: &egui::Ui,
    response: &egui::Response,
) -> Option<acceptance_trace::RadialInsertionWidgetGeometry> {
    let pixels_per_point = ui.ctx().pixels_per_point();
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return None;
    }
    let Some(viewport_rect) = ui.ctx().input(|input| input.viewport().inner_rect) else {
        return None;
    };
    let scale = |coordinate: f32| {
        let value = coordinate * pixels_per_point;
        (value.is_finite() && value >= i32::MIN as f32 && value <= i32::MAX as f32)
            .then(|| value.round() as i32)
    };
    let full = response.rect;
    let clipped = full.intersect(ui.clip_rect());
    if !clipped.is_positive() {
        return None;
    }
    let client_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_rect.size());
    let coordinates = [
        scale(clipped.left())?,
        scale(clipped.top())?,
        scale(clipped.right())?,
        scale(clipped.bottom())?,
        scale(full.left())?,
        scale(full.top())?,
        scale(full.right())?,
        scale(full.bottom())?,
        scale(viewport_rect.width())?,
        scale(viewport_rect.height())?,
    ];
    let bounds = [
        coordinates[0],
        coordinates[1],
        coordinates[2],
        coordinates[3],
    ];
    let full_bounds = [
        coordinates[4],
        coordinates[5],
        coordinates[6],
        coordinates[7],
    ];
    let client_size = [coordinates[8], coordinates[9]];
    let visible = client_size[0] > 0
        && client_size[1] > 0
        && bounds[0] >= 0
        && bounds[1] >= 0
        && bounds[2] > bounds[0]
        && bounds[3] > bounds[1]
        && bounds[2] <= client_size[0]
        && bounds[3] <= client_size[1];
    let fully_visible =
        visible && full == clipped && client_rect.contains_rect(full) && full_bounds == bounds;
    Some(acceptance_trace::RadialInsertionWidgetGeometry {
        bounds,
        full_bounds,
        client_size,
        visible,
        fully_visible,
    })
}

fn cell_content_kind_key(content: &CellContent) -> u8 {
    match content {
        CellContent::Spacer => 0,
        CellContent::Action { .. } => 1,
        CellContent::Submenu { .. } => 2,
        CellContent::Dynamic { .. } => 3,
        CellContent::Control { .. } => 4,
    }
}

fn ring_proposal_token(proposal: &menu::RingEditProposal, nonce: u64) -> String {
    format!(
        "proposal:g{}:n{nonce}:{}:{}",
        proposal.base_generation.0, proposal.menu_id, proposal.ring_id,
    )
}

fn selected_menu_id(session: &RadialAuthoringSession) -> Option<MenuId> {
    match session.selection.as_ref()? {
        StableSelection::Menu(id)
        | StableSelection::Ring { menu_id: id, .. }
        | StableSelection::Cell { menu_id: id, .. } => Some(id.clone()),
        StableSelection::CellSet(cells) => Some(cells.primary.menu_id.clone()),
        _ => None,
    }
}

fn widget_edit_phase(response: &egui::Response) -> EditPhase {
    if response.lost_focus() || response.drag_stopped() {
        EditPhase::End
    } else if response.drag_started() {
        EditPhase::Begin
    } else {
        EditPhase::Update
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct WidgetEditSignals {
    drag_started: bool,
    ended: bool,
}

#[derive(Clone, Debug)]
struct ContinuousWidgetEdit {
    field: String,
    signals: WidgetEditSignals,
}

fn continuous_widget_edit(
    response: &egui::Response,
    field: impl Into<String>,
) -> Option<ContinuousWidgetEdit> {
    (response.changed() || response.lost_focus() || response.drag_stopped()).then(|| {
        ContinuousWidgetEdit {
            field: field.into(),
            signals: WidgetEditSignals::from_response(response),
        }
    })
}

fn set_first_edit<T>(slot: &mut Option<T>, candidate: Option<T>) {
    if slot.is_none() {
        *slot = candidate;
    }
}

impl WidgetEditSignals {
    fn from_response(response: &egui::Response) -> Self {
        Self {
            drag_started: response.drag_started(),
            ended: response.lost_focus() || response.drag_stopped(),
        }
    }

    fn phase(self) -> EditPhase {
        if self.ended {
            EditPhase::End
        } else if self.drag_started {
            EditPhase::Begin
        } else {
            EditPhase::Update
        }
    }
}

fn dispatch_widget_document_edit(
    session: &mut RadialAuthoringSession,
    document: RadialDocument,
    entity: String,
    field: &str,
    signals: WidgetEditSignals,
) -> Result<(), AuthoringError> {
    session.replace_document_edit(
        document,
        EditKey {
            entity,
            field: field.into(),
        },
        signals.phase(),
    )
}

fn style_value_widget(
    ui: &mut egui::Ui,
    kind: skin_editor::StyleControlKind,
    current: &serde_json::Value,
    inherited: &serde_json::Value,
    text_input: &mut String,
    font_families: &[String],
    accessible_name: &str,
    trace: Option<(
        DesignerAuthoringTarget,
        ViewportClass,
        Option<usize>,
        Correlation,
    )>,
    mut ui_pass: Option<&mut RadialUiPass>,
) -> Option<(serde_json::Value, EditPhase)> {
    use skin_editor::StyleControlKind;
    let payload = skin_editor::override_payload(current)
        .or_else(|| skin_editor::override_payload(inherited))
        .unwrap_or(serde_json::Value::Null);
    let changed = match kind {
        StyleControlKind::Toggle => {
            let mut value = payload.as_bool().unwrap_or(false);
            let response = ui.checkbox(&mut value, "");
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Checkbox,
                    ui.is_enabled(),
                    value,
                    accessible_name,
                )
            });
            if let Some((target, viewport, index, correlation)) = trace {
                trace_designer_authoring_control(
                    ui,
                    &response,
                    target,
                    DesignerAuthoringRole::Checkbox,
                    index,
                    true,
                    value,
                    viewport,
                    correlation,
                );
            }
            if let Some(pass) = ui_pass.as_deref()
                && let Some(jump) = pass.jump.as_ref()
            {
                jump.register(
                    ui,
                    &response,
                    &format!("style/{:016x}", radial_value_digest(&accessible_name)),
                    "Style control",
                    TargetKind::Checkbox,
                );
            }
            (response.changed() || response.lost_focus())
                .then(|| (serde_json::json!(value), widget_edit_phase(&response)))
        }
        StyleControlKind::Scalar | StyleControlKind::Opacity => {
            let mut value = payload.as_f64().unwrap_or(1.0) as f32;
            let response = if kind == StyleControlKind::Opacity {
                ui.add(egui::Slider::new(&mut value, 0.0..=1.0))
            } else {
                ui.add(egui::DragValue::new(&mut value).speed(0.05))
            };
            response.widget_info(|| {
                if kind == StyleControlKind::Opacity {
                    egui::WidgetInfo::slider(ui.is_enabled(), f64::from(value), accessible_name)
                } else {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::DragValue,
                        ui.is_enabled(),
                        accessible_name,
                    )
                }
            });
            if let Some(pass) = ui_pass.as_deref()
                && let Some(jump) = pass.jump.as_ref()
            {
                jump.register(
                    ui,
                    &response,
                    &format!("style/{:016x}", radial_value_digest(&accessible_name)),
                    "Style value",
                    TargetKind::Slider,
                );
            }
            (response.changed() || response.drag_stopped() || response.lost_focus())
                .then(|| (serde_json::json!(value), widget_edit_phase(&response)))
        }
        StyleControlKind::Color => {
            let component = |name: &str, fallback| {
                payload
                    .get(name)
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(fallback) as u8
            };
            let original = [
                component("red", 255),
                component("green", 255),
                component("blue", 255),
                component("alpha", 255),
            ];
            let mut color = egui::Color32::from_rgba_unmultiplied(
                original[0],
                original[1],
                original[2],
                original[3],
            );
            let response = ui.color_edit_button_srgba(&mut color);
            let [red, green, blue, alpha] = if response.changed() {
                color.to_srgba_unmultiplied()
            } else {
                original
            };
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::ColorButton,
                    ui.is_enabled(),
                    format!(
                        "{accessible_name}: rgba({}, {}, {}, {})",
                        red, green, blue, alpha
                    ),
                )
            });
            if let Some(pass) = ui_pass.as_deref()
                && let Some(jump) = pass.jump.as_ref()
            {
                jump.register(
                    ui,
                    &response,
                    &format!("style/{:016x}", radial_value_digest(&accessible_name)),
                    "Style color",
                    TargetKind::Button,
                );
            }
            (response.changed() || response.drag_stopped() || response.lost_focus()).then(|| {
                (
                    serde_json::json!({
                        "red": red, "green": green, "blue": blue, "alpha": alpha
                    }),
                    widget_edit_phase(&response),
                )
            })
        }
        StyleControlKind::Offset => {
            let mut x = payload
                .get("x")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32;
            let mut y = payload
                .get("y")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32;
            let x_response = ui.add(egui::DragValue::new(&mut x).prefix("x "));
            let y_response = ui.add(egui::DragValue::new(&mut y).prefix("y "));
            x_response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::DragValue,
                    ui.is_enabled(),
                    format!("{accessible_name} x"),
                )
            });
            y_response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::DragValue,
                    ui.is_enabled(),
                    format!("{accessible_name} y"),
                )
            });
            if let Some(pass) = ui_pass.as_deref()
                && let Some(jump) = pass.jump.as_ref()
            {
                jump.register(
                    ui,
                    &x_response,
                    &format!("style/{:016x}/x", radial_value_digest(&accessible_name)),
                    "Style offset x",
                    TargetKind::Slider,
                );
                jump.register(
                    ui,
                    &y_response,
                    &format!("style/{:016x}/y", radial_value_digest(&accessible_name)),
                    "Style offset y",
                    TargetKind::Slider,
                );
            }
            let response = if x_response.changed() || x_response.drag_stopped() {
                x_response
            } else {
                y_response
            };
            (response.changed() || response.drag_stopped() || response.lost_focus()).then(|| {
                (
                    serde_json::json!({ "x": x, "y": y }),
                    widget_edit_phase(&response),
                )
            })
        }
        StyleControlKind::TooltipMode | StyleControlKind::Quality => {
            let choices: &[(&str, &str)] = if kind == StyleControlKind::TooltipMode {
                &[
                    ("Disabled", "disabled"),
                    ("Explicit", "explicit"),
                    ("Automatic", "automatic"),
                ]
            } else {
                &[
                    ("Fast", "fast"),
                    ("Balanced", "balanced"),
                    ("High quality", "high_quality"),
                ]
            };
            let mut value = payload.as_str().unwrap_or(choices[0].1).to_owned();
            let before = value.clone();
            let combo_id = ui.next_auto_id();
            let combo_identity = format!("combo/{:016x}", ui.make_persistent_id(combo_id).value());
            let combo_jump = ui_pass.as_deref().and_then(|pass| {
                pass.jump
                    .as_ref()
                    .map(|jump| jump.child("combos").combo_popup(&combo_identity))
            });
            let combo = egui::ComboBox::from_id_salt(combo_id)
                .selected_text(
                    choices
                        .iter()
                        .find(|(_, wire)| *wire == value)
                        .map_or(value.as_str(), |(label, _)| label),
                )
                .show_ui(ui, |ui| {
                    for (label, wire) in choices {
                        let wire_identity = radial_value_digest(wire);
                        let option = ui
                            .scope_builder(
                                egui::UiBuilder::new()
                                    .id(ui.id().with(("radial-style-option", wire_identity))),
                                |ui| ui.selectable_value(&mut value, (*wire).to_owned(), *label),
                            )
                            .inner;
                        register_radial_combo_option(
                            combo_jump.as_ref(),
                            ui,
                            &option,
                            &format!("{combo_identity}/option/{wire_identity:016x}"),
                            "Style option",
                        );
                    }
                });
            if let Some(ui_pass) = ui_pass.as_deref_mut() {
                register_radial_combo_anchor(
                    ui_pass,
                    &combo.response,
                    &combo_identity,
                    "Style selector",
                );
                if let Some(jump) = ui_pass.jump.as_ref() {
                    jump.register(
                        ui,
                        &combo.response,
                        &combo_identity,
                        "Style selector",
                        TargetKind::ComboBox,
                    );
                }
            }
            combo.response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::ComboBox,
                    ui.is_enabled(),
                    accessible_name,
                )
            });
            (value != before).then(|| (serde_json::json!(value), EditPhase::Atomic))
        }
        StyleControlKind::Font => {
            if text_input.is_empty() {
                *text_input = payload.as_str().unwrap_or_default().to_owned();
            }
            let before = text_input.clone();
            let combo_id = ui.next_auto_id();
            let combo_identity = format!("combo/{:016x}", ui.make_persistent_id(combo_id).value());
            let combo_jump = ui_pass.as_deref().and_then(|pass| {
                pass.jump
                    .as_ref()
                    .map(|jump| jump.child("combos").combo_popup(&combo_identity))
            });
            let combo = egui::ComboBox::from_id_salt(combo_id)
                .selected_text(if text_input.is_empty() {
                    "System fallback"
                } else {
                    text_input.as_str()
                })
                .show_ui(ui, |ui| {
                    let fallback_id = radial_value_digest(&String::new());
                    let fallback = ui
                        .scope_builder(
                            egui::UiBuilder::new()
                                .id(ui.id().with(("radial-font-option", fallback_id))),
                            |ui| ui.selectable_value(text_input, String::new(), "System fallback"),
                        )
                        .inner;
                    register_radial_combo_option(
                        combo_jump.as_ref(),
                        ui,
                        &fallback,
                        &format!("{combo_identity}/option/{fallback_id:016x}"),
                        "Font family option",
                    );
                    for family in font_families {
                        let family_id = radial_value_digest(family);
                        let option = ui
                            .scope_builder(
                                egui::UiBuilder::new()
                                    .id(ui.id().with(("radial-font-option", family_id))),
                                |ui| ui.selectable_value(text_input, family.clone(), family),
                            )
                            .inner;
                        register_radial_combo_option(
                            combo_jump.as_ref(),
                            ui,
                            &option,
                            &format!("{combo_identity}/option/{family_id:016x}"),
                            "Font family option",
                        );
                    }
                });
            if let Some(ui_pass) = ui_pass.as_deref_mut() {
                register_radial_combo_anchor(
                    ui_pass,
                    &combo.response,
                    &combo_identity,
                    "Font selector",
                );
                if let Some(jump) = ui_pass.jump.as_ref() {
                    jump.register(
                        ui,
                        &combo.response,
                        &combo_identity,
                        "Font selector",
                        TargetKind::ComboBox,
                    );
                }
            }
            combo.response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::ComboBox,
                    ui.is_enabled(),
                    accessible_name,
                )
            });
            let combo_changed = *text_input != before;
            let response = ui.text_edit_singleline(text_input);
            response.widget_info(|| {
                let mut info =
                    egui::WidgetInfo::text_edit(ui.is_enabled(), "", text_input.as_str(), "");
                info.label = Some(accessible_name.into());
                info
            });
            if let Some(pass) = ui_pass.as_deref()
                && let Some(jump) = pass.jump.as_ref()
            {
                jump.register(
                    ui,
                    &response,
                    &format!(
                        "style/{:016x}/font-name",
                        radial_value_digest(&accessible_name)
                    ),
                    "Font family",
                    TargetKind::TextEntry,
                );
            }
            if response.changed() || response.lost_focus() {
                Some((
                    serde_json::json!(text_input.clone()),
                    widget_edit_phase(&response),
                ))
            } else if combo_changed {
                Some((serde_json::json!(text_input.clone()), EditPhase::Atomic))
            } else {
                None
            }
        }
        StyleControlKind::Text => {
            if text_input.is_empty() {
                *text_input = payload.as_str().unwrap_or_default().to_owned();
            }
            let response = ui.text_edit_singleline(text_input);
            response.widget_info(|| {
                let mut info =
                    egui::WidgetInfo::text_edit(ui.is_enabled(), "", text_input.as_str(), "");
                info.label = Some(accessible_name.into());
                info
            });
            if let Some(pass) = ui_pass.as_deref()
                && let Some(jump) = pass.jump.as_ref()
            {
                jump.register(
                    ui,
                    &response,
                    &format!("style/{:016x}/text", radial_value_digest(&accessible_name)),
                    "Style text",
                    TargetKind::TextEntry,
                );
            }
            (response.changed() || response.lost_focus()).then(|| {
                (
                    serde_json::json!(text_input.clone()),
                    widget_edit_phase(&response),
                )
            })
        }
        StyleControlKind::Resource => {
            if text_input.is_empty() {
                *text_input = payload
                    .get("path")
                    .or_else(|| payload.get("file_name"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
            }
            let path_response = ui
                .text_edit_singleline(text_input)
                .on_hover_text("Search-path file name or icon resource path");
            path_response.widget_info(|| {
                let mut info =
                    egui::WidgetInfo::text_edit(ui.is_enabled(), "", text_input.as_str(), "");
                info.label = Some(accessible_name.into());
                info
            });
            if let Some(pass) = ui_pass.as_deref()
                && let Some(jump) = pass.jump.as_ref()
            {
                jump.register(
                    ui,
                    &path_response,
                    &format!(
                        "style/{:016x}/resource",
                        radial_value_digest(&accessible_name)
                    ),
                    "Style resource path",
                    TargetKind::TextEntry,
                );
            }
            let mut resource = payload.clone();
            if let Some(object) = resource.as_object_mut() {
                if object.contains_key("path") {
                    object.insert("path".into(), serde_json::json!(text_input.clone()));
                } else if object.contains_key("file_name") {
                    object.insert("file_name".into(), serde_json::json!(text_input.clone()));
                }
            }
            let mut response = continuous_widget_edit(&path_response, "resource.path")
                .map(|edit| (resource.clone(), edit.signals.phase()));
            if let Some(index) = resource.get("index").and_then(serde_json::Value::as_i64) {
                let mut index = i32::try_from(index).unwrap_or(1).max(1);
                let index_response =
                    ui.add(egui::DragValue::new(&mut index).prefix("Resource index "));
                index_response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::DragValue,
                        ui.is_enabled(),
                        format!("{accessible_name} resource index"),
                    )
                });
                if let Some(pass) = ui_pass.as_deref()
                    && let Some(jump) = pass.jump.as_ref()
                {
                    jump.register(
                        ui,
                        &index_response,
                        &format!(
                            "style/{:016x}/resource-index",
                            radial_value_digest(&accessible_name)
                        ),
                        "Style resource index",
                        TargetKind::Slider,
                    );
                }
                if let Some(object) = resource.as_object_mut() {
                    object.insert("index".into(), serde_json::json!(index));
                }
                response = response.or_else(|| {
                    continuous_widget_edit(&index_response, "resource.index")
                        .map(|edit| (resource, edit.signals.phase()))
                });
            }
            response
        }
    };
    changed.map(|(value, phase)| (skin_editor::with_payload(value), phase))
}

fn persisted_section(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    key: &str,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
    jump: Option<RadialJumpPass>,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let open = expanded_sections.get(key).copied().unwrap_or(false);
    let response = egui::CollapsingHeader::new(label)
        .id_salt(("radial-designer-section", key))
        .default_open(open)
        .open(Some(open))
        .show(ui, body);
    if let Some(jump) = jump {
        jump.register(
            ui,
            &response.header_response,
            &format!("section/{:016x}", radial_value_digest(&key)),
            "Inspector section",
            TargetKind::Button,
        );
    }
    if response.header_response.changed() {
        expanded_sections.insert(key.to_owned(), !open);
        true
    } else {
        false
    }
}

fn menu_behavior_controls(
    ui: &mut egui::Ui,
    menu: &mut crate::radial::model::MenuDefinition,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
    correlation: Correlation,
    ui_pass: &mut RadialUiPass,
) -> Option<ContinuousWidgetEdit> {
    let jump = ui_pass.child_jump("menu-behavior");
    let mut continuous = None;
    enum_combo(
        ui,
        "Layout",
        &mut menu.layout,
        &[
            ("Circular cells", LayoutKind::CircularCells),
            ("Wedges", LayoutKind::Wedges),
        ],
        ui_pass,
    );
    enum_combo(
        ui,
        "Interaction",
        &mut menu.interaction,
        &[
            ("Sticky click", InteractionMode::StickyClick),
            ("Release to select", InteractionMode::ReleaseToSelect),
            ("Hold and click", InteractionMode::HoldAndClick),
        ],
        ui_pass,
    );
    let mut dwell_enabled = menu.hover_dwell_ms.is_some();
    let dwell = ui.checkbox(&mut dwell_enabled, "Hover dwell");
    if let Some(jump) = jump.as_ref() {
        jump.register(
            ui,
            &dwell,
            "hover-dwell-enabled",
            "Hover dwell",
            TargetKind::Checkbox,
        );
    }
    if dwell.changed() {
        menu.hover_dwell_ms = dwell_enabled.then_some(350);
    }
    if let Some(dwell) = menu.hover_dwell_ms.as_mut() {
        let response = ui.add(
            egui::DragValue::new(dwell)
                .prefix("Dwell ms ")
                .range(1..=10_000),
        );
        if let Some(jump) = jump.as_ref() {
            jump.register(
                ui,
                &response,
                "hover-dwell-ms",
                "Hover dwell duration",
                TargetKind::Slider,
            );
        }
        continuous = continuous_widget_edit(&response, "hover_dwell_ms");
    }
    enum_combo(
        ui,
        "Open child menus",
        &mut menu.submenu_presentation,
        &[
            ("Cascade", SubmenuPresentation::Cascade),
            ("Same center", SubmenuPresentation::SameCenter),
        ],
        ui_pass,
    );
    traced_menu_after_action_combo(ui, &mut menu.after_action, correlation, ui_pass);
    let center_response = ui.add(
        egui::DragValue::new(&mut menu.center_radius)
            .prefix("Center radius ")
            .speed(1.0),
    );
    if let Some(jump) = jump.as_ref() {
        jump.register(
            ui,
            &center_response,
            "center-radius",
            "Center radius",
            TargetKind::Slider,
        );
    }
    set_first_edit(
        &mut continuous,
        continuous_widget_edit(&center_response, "center_radius"),
    );
    let mirror = ui.checkbox(
        &mut menu.mirror_primary_to_secondary,
        "Mirror primary to secondary",
    );
    if let Some(jump) = jump.as_ref() {
        jump.register(
            ui,
            &mirror,
            "mirror-primary-secondary",
            "Mirror primary action to secondary",
            TargetKind::Checkbox,
        );
    }
    persisted_section(
        ui,
        "Center mappings",
        &format!("menu:{}:center-mappings", menu.id),
        expanded_sections,
        ui_pass.child_jump("sections"),
        |ui| {
            let before = menu.center_control;
            optional_control_combo(ui, "Primary control", &mut menu.center_control, ui_pass);
            if menu.center_control.is_some() && menu.center_control != before {
                menu.center_action = None;
            }
            if menu.center_action.is_some() {
                let clear = ui.button("Clear primary action");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &clear,
                        "center-clear-primary",
                        "Clear primary action",
                        TargetKind::Button,
                    );
                }
                if clear.clicked() {
                    menu.center_action = None;
                }
            }
            after_action_combo(
                ui,
                "Primary after action",
                &mut menu.center_primary_after_action,
                ui_pass,
            );
            let before = menu.center_secondary_control;
            optional_control_combo(
                ui,
                "Secondary control",
                &mut menu.center_secondary_control,
                ui_pass,
            );
            if menu.center_secondary_control.is_some() && menu.center_secondary_control != before {
                menu.center_secondary_action = None;
            }
            if menu.center_secondary_action.is_some() {
                let clear = ui.button("Clear secondary action");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &clear,
                        "center-clear-secondary",
                        "Clear secondary action",
                        TargetKind::Button,
                    );
                }
                if clear.clicked() {
                    menu.center_secondary_action = None;
                }
            }
            after_action_combo(
                ui,
                "Secondary after action",
                &mut menu.center_secondary_after_action,
                ui_pass,
            );
            ui.small(if menu.center_action.is_some() {
                "Primary action assigned"
            } else {
                "No primary action"
            });
            ui.small(if menu.center_secondary_action.is_some() {
                "Secondary action assigned"
            } else {
                "No secondary action"
            });
        },
    );
    persisted_section(
        ui,
        "Background mappings",
        &format!("menu:{}:background-mappings", menu.id),
        expanded_sections,
        ui_pass.child_jump("sections"),
        |ui| {
            let before = menu.background_control;
            optional_control_combo(ui, "Primary control", &mut menu.background_control, ui_pass);
            if menu.background_control.is_some() && menu.background_control != before {
                menu.background_action = None;
            }
            if menu.background_action.is_some() {
                let clear = ui.button("Clear primary action");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &clear,
                        "background-clear-primary",
                        "Clear primary action",
                        TargetKind::Button,
                    );
                }
                if clear.clicked() {
                    menu.background_action = None;
                }
            }
            after_action_combo(
                ui,
                "Primary after action",
                &mut menu.background_primary_after_action,
                ui_pass,
            );
            let before = menu.background_secondary_control;
            optional_control_combo(
                ui,
                "Secondary control",
                &mut menu.background_secondary_control,
                ui_pass,
            );
            if menu.background_secondary_control.is_some()
                && menu.background_secondary_control != before
            {
                menu.background_secondary_action = None;
            }
            if menu.background_secondary_action.is_some() {
                let clear = ui.button("Clear secondary action");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &clear,
                        "background-clear-secondary",
                        "Clear secondary action",
                        TargetKind::Button,
                    );
                }
                if clear.clicked() {
                    menu.background_secondary_action = None;
                }
            }
            after_action_combo(
                ui,
                "Secondary after action",
                &mut menu.background_secondary_after_action,
                ui_pass,
            );
            ui.small(if menu.background_action.is_some() {
                "Primary action assigned"
            } else {
                "No primary action"
            });
            ui.small(if menu.background_secondary_action.is_some() {
                "Secondary action assigned"
            } else {
                "No secondary action"
            });
        },
    );
    continuous
}

fn enum_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    choices: &[(&str, T)],
    ui_pass: &mut RadialUiPass,
) {
    let selected = choices
        .iter()
        .find(|(_, item)| item == value)
        .map_or("Unknown", |(label, _)| *label);
    let combo_identity = format!("combo/{:016x}", ui.make_persistent_id(label).value());
    let combo_jump = ui_pass
        .jump
        .as_ref()
        .map(|pass| pass.child("combos").combo_popup(&combo_identity));
    let combo = egui::ComboBox::from_label(label)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (index, (label, choice)) in choices.iter().enumerate() {
                let option = ui
                    .scope_builder(
                        egui::UiBuilder::new().id(ui.id().with(("radial-enum-option", index))),
                        |ui| ui.selectable_value(value, *choice, *label),
                    )
                    .inner;
                register_radial_combo_option(
                    combo_jump.as_ref(),
                    ui,
                    &option,
                    &format!("{combo_identity}/option/{index}"),
                    "Choice",
                );
            }
        });
    register_radial_combo_anchor(ui_pass, &combo.response, &combo_identity, "Choice selector");
    if let Some(pass) = ui_pass.jump.as_ref().map(|pass| pass.child("combos")) {
        pass.register(
            ui,
            &combo.response,
            &combo_identity,
            "Choice selector",
            TargetKind::ComboBox,
        );
    }
}

const AFTER_ACTION_CHOICES: [(&str, AfterActionPolicy); 4] = [
    ("Inherit", AfterActionPolicy::Inherit),
    ("Keep open", AfterActionPolicy::KeepOpen),
    ("Close current", AfterActionPolicy::CloseCurrentMenu),
    ("Close tree", AfterActionPolicy::CloseTree),
];

fn after_action_combo(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut AfterActionPolicy,
    ui_pass: &mut RadialUiPass,
) {
    enum_combo(ui, label, value, &AFTER_ACTION_CHOICES, ui_pass);
}

fn traced_menu_after_action_combo(
    ui: &mut egui::Ui,
    value: &mut AfterActionPolicy,
    correlation: Correlation,
    ui_pass: &mut RadialUiPass,
) {
    let selected = AFTER_ACTION_CHOICES
        .iter()
        .find(|(_, choice)| choice == value)
        .map_or("Unknown", |(label, _)| *label);
    let combo_identity = format!(
        "combo/{:016x}",
        ui.make_persistent_id("Menu after action").value()
    );
    let combo_jump = ui_pass
        .jump
        .as_ref()
        .map(|pass| pass.child("combos").combo_popup(&combo_identity));
    let response = egui::ComboBox::from_label("Menu after action")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (index, (label, choice)) in AFTER_ACTION_CHOICES.iter().copied().enumerate() {
                let option = ui
                    .scope_builder(
                        egui::UiBuilder::new()
                            .id(ui.id().with(("radial-after-action-option", index))),
                        |ui| ui.selectable_value(value, choice, label),
                    )
                    .inner;
                register_radial_combo_option(
                    combo_jump.as_ref(),
                    ui,
                    &option,
                    &format!("{combo_identity}/option/{index}"),
                    "After action option",
                );
                trace_designer_authoring_control(
                    ui,
                    &option,
                    DesignerAuthoringTarget::AfterActionOption,
                    DesignerAuthoringRole::Selectable,
                    Some(index),
                    true,
                    *value == choice,
                    ViewportClass::Deferred,
                    correlation,
                );
            }
        });
    register_radial_combo_anchor(
        ui_pass,
        &response.response,
        &combo_identity,
        "After action selector",
    );
    if let Some(pass) = ui_pass.jump.as_ref().map(|pass| pass.child("combos")) {
        pass.register(
            ui,
            &response.response,
            &combo_identity,
            "After action selector",
            TargetKind::ComboBox,
        );
    }
    if !acceptance_trace::enabled() {
        return;
    }
    // The labelled ComboBox response includes its separate label. Its ID still
    // belongs to the button registered this frame, which opens the popup.
    let Some(button_response) = ui.ctx().read_response(response.response.id) else {
        return;
    };
    trace_designer_authoring_control(
        ui,
        &button_response,
        DesignerAuthoringTarget::MenuAfterAction,
        DesignerAuthoringRole::ComboBox,
        None,
        true,
        false,
        ViewportClass::Deferred,
        correlation,
    );
}

fn optional_control_combo(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<Control>,
    ui_pass: &mut RadialUiPass,
) {
    enum_combo(
        ui,
        label,
        value,
        &[
            ("None", None),
            ("Back", Some(Control::Back)),
            ("Close", Some(Control::Close)),
            ("Next page", Some(Control::NextPage)),
            ("Previous page", Some(Control::PreviousPage)),
            ("Drag", Some(Control::Drag)),
        ],
        ui_pass,
    );
}

fn cell_content_controls(
    ui: &mut egui::Ui,
    document: &RadialDocument,
    cell: &mut crate::radial::model::CellDefinition,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
    ui_pass: &mut RadialUiPass,
) -> Option<ContinuousWidgetEdit> {
    let jump = ui_pass.jump.clone();
    let mut continuous = None;
    let dynamic_source = ui.button("Dynamic source");
    if let Some(jump) = jump.as_ref() {
        jump.register(
            ui,
            &dynamic_source,
            "content/dynamic-source",
            "Dynamic source",
            TargetKind::Button,
        );
    }
    if dynamic_source.clicked() && !matches!(cell.content, CellContent::Dynamic { .. }) {
        cell.content = CellContent::Dynamic {
            source: DynamicSource::Favorites,
        };
    }
    let control = ui.button("Control");
    if let Some(jump) = jump.as_ref() {
        jump.register(
            ui,
            &control,
            "content/control",
            "Control content",
            TargetKind::Button,
        );
    }
    if control.clicked() && !matches!(cell.content, CellContent::Control { .. }) {
        cell.content = CellContent::Control {
            control: Control::Back,
        };
    }
    let submenu = ui.button("Submenu");
    if let Some(jump) = jump.as_ref() {
        jump.register(
            ui,
            &submenu,
            "content/submenu",
            "Submenu content",
            TargetKind::Button,
        );
    }
    if submenu.clicked()
        && !matches!(cell.content, CellContent::Submenu { .. })
        && let Some(menu) = document.menus.first()
    {
        cell.content = CellContent::Submenu {
            menu_id: menu.id.clone(),
        };
    }
    match &mut cell.content {
        CellContent::Dynamic { source } => {
            let mut key = dynamic_source_key(source);
            enum_combo(
                ui,
                "Source",
                &mut key,
                &[
                    ("Favorites", 0),
                    ("Recent", 1),
                    ("Clipboard", 2),
                    ("Snippets", 3),
                    ("Notes", 4),
                    ("Windows", 5),
                    ("Macros", 6),
                    ("Applications", 7),
                    ("Dashboard", 8),
                    ("Launcher results", 9),
                    ("Launcher query", 10),
                ],
                ui_pass,
            );
            if key != dynamic_source_key(source) {
                *source = dynamic_source_for_key(key);
            }
            match source {
                DynamicSource::LauncherResults { max_items } => {
                    let response = ui.add(
                        egui::DragValue::new(max_items)
                            .prefix("Maximum ")
                            .range(1..=100),
                    );
                    if let Some(jump) = jump.as_ref() {
                        jump.register(
                            ui,
                            &response,
                            "dynamic/max-items",
                            "Maximum results",
                            TargetKind::Slider,
                        );
                    }
                    continuous = continuous_widget_edit(&response, "dynamic.max_items");
                }
                DynamicSource::LauncherQuery { query, max_items } => {
                    let query_response = ui
                        .text_edit_singleline(query)
                        .on_hover_text("Read-only launcher query");
                    if let Some(jump) = jump.as_ref() {
                        jump.register(
                            ui,
                            &query_response,
                            "dynamic/query",
                            "Launcher query",
                            TargetKind::TextEntry,
                        );
                    }
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&query_response, "dynamic.query"),
                    );
                    let maximum_response = ui.add(
                        egui::DragValue::new(max_items)
                            .prefix("Maximum ")
                            .range(1..=100),
                    );
                    if let Some(jump) = jump.as_ref() {
                        jump.register(
                            ui,
                            &maximum_response,
                            "dynamic/max-items",
                            "Maximum results",
                            TargetKind::Slider,
                        );
                    }
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&maximum_response, "dynamic.max_items"),
                    );
                }
                _ => {}
            }
            ui.small("Runtime status is shown truthfully as loading, unavailable, empty, action, or manage rows.");
        }
        CellContent::Control { control } => enum_combo(
            ui,
            "Control",
            control,
            &[
                ("Back", Control::Back),
                ("Close", Control::Close),
                ("Next page", Control::NextPage),
                ("Previous page", Control::PreviousPage),
                ("Drag", Control::Drag),
            ],
            ui_pass,
        ),
        CellContent::Submenu { menu_id } => {
            let combo_identity = format!(
                "combo/{:016x}",
                ui.make_persistent_id("Link existing submenu").value()
            );
            let combo_jump = ui_pass
                .jump
                .as_ref()
                .map(|pass| pass.child("combos").combo_popup(&combo_identity));
            let submenu_combo = egui::ComboBox::from_label("Link existing submenu")
                .selected_text(
                    document
                        .menus
                        .iter()
                        .find(|menu| menu.id == *menu_id)
                        .map_or_else(|| menu_id.to_string(), |menu| menu.name.clone()),
                )
                .show_ui(ui, |ui| {
                    for menu in &document.menus {
                        let option = ui
                            .scope_builder(
                                egui::UiBuilder::new().id(ui.id().with((
                                    "radial-submenu-option",
                                    radial_value_digest(&menu.id),
                                ))),
                                |ui| ui.selectable_value(menu_id, menu.id.clone(), &menu.name),
                            )
                            .inner;
                        register_radial_combo_option(
                            combo_jump.as_ref(),
                            ui,
                            &option,
                            &format!(
                                "{combo_identity}/option/{:016x}",
                                radial_value_digest(&menu.id)
                            ),
                            "Submenu option",
                        );
                    }
                });
            ui_pass.record_combo(&submenu_combo.response);
            if let Some(jump) = ui_pass.jump.as_ref() {
                jump.register(
                    ui,
                    &submenu_combo.response,
                    &combo_identity,
                    "Link existing submenu",
                    TargetKind::ComboBox,
                );
            }
            ui.small("Use ‘Duplicate subtree’ on the linked menu to create an independent reusable submenu graph.");
        }
        CellContent::Action { .. } => {
            ui.small("Primary action is assigned from the Universal Action picker below.");
        }
        CellContent::Spacer => {
            ui.small("Spacer (no action)");
        }
    };
    persisted_section(
        ui,
        "Alternate clicks and controls",
        &format!("cell:{}:alternate-controls", cell.id),
        expanded_sections,
        ui_pass.child_jump("sections"),
        |ui| {
            let mut remove_click = None;
            for (index, binding) in cell.alternate_clicks.iter_mut().enumerate() {
                enum_combo(
                    ui,
                    "Gesture",
                    &mut binding.gesture,
                    &[
                        ("Primary", crate::radial::model::ClickGesture::Primary),
                        ("Secondary", crate::radial::model::ClickGesture::Secondary),
                        (
                            "Ctrl+primary",
                            crate::radial::model::ClickGesture::CtrlPrimary,
                        ),
                        (
                            "Shift+primary",
                            crate::radial::model::ClickGesture::ShiftPrimary,
                        ),
                        (
                            "Alt+primary",
                            crate::radial::model::ClickGesture::AltPrimary,
                        ),
                    ],
                    ui_pass,
                );
                after_action_combo(ui, "After action", &mut binding.after_action, ui_pass);
                if ui.button("Remove alternate action").clicked() {
                    remove_click = Some(index);
                }
            }
            if let Some(index) = remove_click {
                cell.alternate_clicks.remove(index);
            }
            let mut remove_control = None;
            for (index, binding) in cell.alternate_controls.iter_mut().enumerate() {
                enum_combo(
                    ui,
                    "Control gesture",
                    &mut binding.gesture,
                    &[
                        ("Secondary", crate::radial::model::ClickGesture::Secondary),
                        (
                            "Ctrl+primary",
                            crate::radial::model::ClickGesture::CtrlPrimary,
                        ),
                    ],
                    ui_pass,
                );
                enum_combo(
                    ui,
                    "Control",
                    &mut binding.control,
                    &[
                        ("Back", Control::Back),
                        ("Close", Control::Close),
                        ("Drag", Control::Drag),
                    ],
                    ui_pass,
                );
                if ui.button("Remove alternate control").clicked() {
                    remove_control = Some(index);
                }
            }
            if let Some(index) = remove_control {
                cell.alternate_controls.remove(index);
            }
            if ui.button("Add alternate control").clicked() {
                let gesture = [
                    crate::radial::model::ClickGesture::Secondary,
                    crate::radial::model::ClickGesture::CtrlPrimary,
                    crate::radial::model::ClickGesture::ShiftPrimary,
                    crate::radial::model::ClickGesture::AltPrimary,
                ]
                .into_iter()
                .find(|gesture| {
                    !cell
                        .alternate_controls
                        .iter()
                        .any(|entry| entry.gesture == *gesture)
                        && !cell
                            .alternate_clicks
                            .iter()
                            .any(|entry| entry.gesture == *gesture)
                });
                if let Some(gesture) = gesture {
                    cell.alternate_controls
                        .push(crate::radial::model::ControlClickBinding {
                            gesture,
                            control: Control::Back,
                        });
                }
            }
            let control_gestures = cell
                .alternate_controls
                .iter()
                .map(|entry| entry.gesture)
                .collect::<std::collections::BTreeSet<_>>();
            cell.alternate_clicks
                .retain(|entry| !control_gestures.contains(&entry.gesture));
        },
    );
    continuous
}

fn dynamic_source_key(source: &DynamicSource) -> u8 {
    match source {
        DynamicSource::Favorites => 0,
        DynamicSource::RecentItems => 1,
        DynamicSource::Clipboard => 2,
        DynamicSource::Snippets => 3,
        DynamicSource::Notes => 4,
        DynamicSource::Windows => 5,
        DynamicSource::Macros => 6,
        DynamicSource::Applications => 7,
        DynamicSource::Dashboard => 8,
        DynamicSource::LauncherResults { .. } => 9,
        DynamicSource::LauncherQuery { .. } => 10,
    }
}

fn dynamic_source_for_key(key: u8) -> DynamicSource {
    match key {
        0 => DynamicSource::Favorites,
        1 => DynamicSource::RecentItems,
        2 => DynamicSource::Clipboard,
        3 => DynamicSource::Snippets,
        4 => DynamicSource::Notes,
        5 => DynamicSource::Windows,
        6 => DynamicSource::Macros,
        7 => DynamicSource::Applications,
        8 => DynamicSource::Dashboard,
        9 => DynamicSource::LauncherResults { max_items: 12 },
        _ => DynamicSource::LauncherQuery {
            query: String::new(),
            max_items: 12,
        },
    }
}

fn cell_media_controls(
    ui: &mut egui::Ui,
    document: &RadialDocument,
    cell: &mut crate::radial::model::CellDefinition,
    intent_bridge: &DesignerIntentBridge,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
    menu_id: &MenuId,
    ring_id: &RingId,
    cell_id: &CellId,
    ui_pass: &mut RadialUiPass,
) -> Option<ContinuousWidgetEdit> {
    use crate::radial::model::{MediaKind, MediaReference, Override};
    let jump = ui_pass.jump.clone();
    let mut continuous = None;
    persisted_section(
        ui,
        "Icon and tooltip",
        &format!("cell:{}:icon-tooltip", cell.id),
        expanded_sections,
        ui_pass.child_jump("sections"),
        |ui| {
            ui.horizontal(|ui| {
                ui.label("Tooltip");
                let inherit = ui.button("Inherit");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &inherit,
                        "tooltip/inherit",
                        "Inherit tooltip",
                        TargetKind::Button,
                    );
                }
                if inherit.clicked() {
                    cell.tooltip = Override::Inherit;
                }
                let clear = ui.button("Clear");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &clear,
                        "tooltip/clear",
                        "Clear tooltip",
                        TargetKind::Button,
                    );
                }
                if clear.clicked() {
                    cell.tooltip = Override::Clear;
                }
                let set = ui.button("Set");
                if let Some(jump) = jump.as_ref() {
                    jump.register(ui, &set, "tooltip/set", "Set tooltip", TargetKind::Button);
                }
                if set.clicked() && !matches!(cell.tooltip, Override::Value(_)) {
                    cell.tooltip = Override::Value(cell.label.clone());
                }
                if let Override::Value(value) = &mut cell.tooltip {
                    let response = ui.text_edit_singleline(value);
                    if let Some(jump) = jump.as_ref() {
                        jump.register(
                            ui,
                            &response,
                            "tooltip/text",
                            "Tooltip text",
                            TargetKind::TextEntry,
                        );
                    }
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&response, "tooltip"),
                    );
                }
            });
            ui.horizontal(|ui| {
                ui.label("Icon");
                let inherit = ui.button("Inherit");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &inherit,
                        "icon/inherit",
                        "Inherit icon",
                        TargetKind::Button,
                    );
                }
                if inherit.clicked() {
                    cell.icon = Override::Inherit;
                }
                let clear = ui.button("Clear");
                if let Some(jump) = jump.as_ref() {
                    jump.register(ui, &clear, "icon/clear", "Clear icon", TargetKind::Button);
                }
                if clear.clicked() {
                    cell.icon = Override::Clear;
                }
                let external = ui.button("External file");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &external,
                        "icon/external-file",
                        "Choose external icon",
                        TargetKind::Button,
                    );
                }
                if external.clicked() {
                    intent_bridge.enqueue_file_dialog(
                        DesignerFileDialogRequest::CellExternalIcon {
                            menu_id: menu_id.clone(),
                            ring_id: ring_id.clone(),
                            cell_id: cell_id.clone(),
                        },
                    );
                }
                let search_path = ui.button("Search path");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &search_path,
                        "icon/search-path",
                        "Use icon search path",
                        TargetKind::Button,
                    );
                }
                if search_path.clicked() {
                    cell.icon = Override::Value(MediaReference::SearchPath {
                        file_name: "icon.png".into(),
                    });
                }
                let icon_resource = ui.button("Icon resource");
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &icon_resource,
                        "icon/resource",
                        "Use icon resource",
                        TargetKind::Button,
                    );
                }
                if icon_resource.clicked() {
                    cell.icon = Override::Value(MediaReference::IconResource {
                        path: "shell32.dll".into(),
                        index: 1,
                    });
                }
            });
            let managed: Vec<_> = document
                .assets
                .iter()
                .filter(|asset| asset.kind == MediaKind::Image)
                .map(|asset| asset.id.clone())
                .collect();
            if !managed.is_empty() {
                let mut selected = match &cell.icon {
                    Override::Value(MediaReference::Managed { asset_id }) => Some(asset_id.clone()),
                    _ => None,
                };
                let combo_identity = format!(
                    "combo/{:016x}",
                    ui.make_persistent_id("Managed icon").value()
                );
                let combo_jump = ui_pass
                    .jump
                    .as_ref()
                    .map(|pass| pass.child("combos").combo_popup(&combo_identity));
                let managed_icon_combo = egui::ComboBox::from_label("Managed icon")
                    .selected_text(selected.as_ref().map_or("Choose asset", |id| id.as_str()))
                    .show_ui(ui, |ui| {
                        for id in managed {
                            let choice = Some(id.clone());
                            let choice_identity = radial_value_digest(&choice);
                            let option = ui
                                .scope_builder(
                                    egui::UiBuilder::new().id(ui
                                        .id()
                                        .with(("radial-managed-icon-option", choice_identity))),
                                    |ui| {
                                        ui.selectable_value(
                                            &mut selected,
                                            choice.clone(),
                                            id.as_str(),
                                        )
                                    },
                                )
                                .inner;
                            register_radial_combo_option(
                                combo_jump.as_ref(),
                                ui,
                                &option,
                                &format!("{combo_identity}/option/{choice_identity:016x}"),
                                "Managed icon option",
                            );
                        }
                    });
                ui_pass.record_combo(&managed_icon_combo.response);
                if let Some(jump) = ui_pass.jump.as_ref() {
                    jump.register(
                        ui,
                        &managed_icon_combo.response,
                        &combo_identity,
                        "Managed icon selector",
                        TargetKind::ComboBox,
                    );
                }
                if let Some(asset_id) = selected {
                    cell.icon = Override::Value(MediaReference::Managed { asset_id });
                }
            }
            match &mut cell.icon {
                Override::Value(MediaReference::SearchPath { file_name }) => {
                    let response = ui.text_edit_singleline(file_name);
                    if let Some(jump) = jump.as_ref() {
                        jump.register(
                            ui,
                            &response,
                            "icon/search-path-name",
                            "Search path file name",
                            TargetKind::TextEntry,
                        );
                    }
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&response, "icon.search_path"),
                    );
                }
                Override::Value(MediaReference::ExternalFile { path }) => {
                    let response = ui.text_edit_singleline(path);
                    if let Some(jump) = jump.as_ref() {
                        jump.register(
                            ui,
                            &response,
                            "icon/external-path",
                            "External icon path",
                            TargetKind::TextEntry,
                        );
                    }
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&response, "icon.external_path"),
                    );
                }
                Override::Value(MediaReference::IconResource { path, index }) => {
                    let path_response = ui.text_edit_singleline(path);
                    if let Some(jump) = jump.as_ref() {
                        jump.register(
                            ui,
                            &path_response,
                            "icon/resource-path",
                            "Icon resource path",
                            TargetKind::TextEntry,
                        );
                    }
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&path_response, "icon.resource_path"),
                    );
                    let index_response =
                        ui.add(egui::DragValue::new(index).prefix("Resource index "));
                    if let Some(jump) = jump.as_ref() {
                        jump.register(
                            ui,
                            &index_response,
                            "icon/resource-index",
                            "Icon resource index",
                            TargetKind::Slider,
                        );
                    }
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&index_response, "icon.resource_index"),
                    );
                }
                _ => {}
            }
        },
    );
    continuous
}

fn cell_trigger_controls(
    ui: &mut egui::Ui,
    session: &mut RadialAuthoringSession,
    cell: &mut crate::radial::model::CellDefinition,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
    ui_pass: &mut RadialUiPass,
) -> Option<ContinuousWidgetEdit> {
    let jump = ui_pass.jump.clone();
    let mut continuous = None;
    persisted_section(
        ui,
        "Shortcuts and hotstrings",
        &format!("cell:{}:shortcuts-hotstrings", cell.id),
        expanded_sections,
        ui_pass.child_jump("sections"),
        |ui| {
            let mut remove_shortcut = None;
            for (index, shortcut) in cell.shortcuts.iter_mut().enumerate() {
                let shortcut_identity = radial_value_digest(&shortcut.id);
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id(ui.id().with(("radial-shortcut-row", shortcut_identity))),
                    |ui| {
                        let response = ui.text_edit_singleline(&mut shortcut.chord);
                        if let Some(jump) = jump.as_ref() {
                            jump.register(
                                ui,
                                &response,
                                &format!("shortcut/{shortcut_identity:016x}/chord"),
                                "Shortcut chord",
                                TargetKind::TextEntry,
                            );
                        }
                        set_first_edit(
                            &mut continuous,
                            continuous_widget_edit(
                                &response,
                                format!("shortcut:{}.chord", shortcut.id),
                            ),
                        );
                        enum_combo(
                            ui,
                            "Scope",
                            &mut shortcut.scope,
                            &[
                                ("Menu local", crate::radial::model::TriggerScope::MenuLocal),
                                ("Global", crate::radial::model::TriggerScope::Global),
                            ],
                            ui_pass,
                        );
                        let remove = ui.button("Remove shortcut");
                        if let Some(jump) = jump.as_ref() {
                            jump.register(
                                ui,
                                &remove,
                                &format!("shortcut/{shortcut_identity:016x}/remove"),
                                "Remove shortcut",
                                TargetKind::Button,
                            );
                        }
                        if remove.clicked() {
                            remove_shortcut = Some(index);
                        }
                    },
                );
            }
            if let Some(index) = remove_shortcut {
                cell.shortcuts.remove(index);
            }
            let add_shortcut = ui.button("Add shortcut");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &add_shortcut,
                    "shortcut/add",
                    "Add shortcut",
                    TargetKind::Button,
                );
            }
            if add_shortcut.clicked() {
                cell.shortcuts.push(crate::radial::model::ItemShortcut {
                    id: session.allocate_shortcut_id("shortcut"),
                    chord: "Ctrl+Alt+1".into(),
                    gesture: crate::radial::model::ClickGesture::Primary,
                    scope: crate::radial::model::TriggerScope::MenuLocal,
                });
            }
            let mut remove_hotstring = None;
            for (index, hotstring) in cell.hotstrings.iter_mut().enumerate() {
                let hotstring_identity = radial_value_digest(&hotstring.id);
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id(ui.id().with(("radial-hotstring-row", hotstring_identity))),
                    |ui| {
                        let response = ui.text_edit_singleline(&mut hotstring.text);
                        if let Some(jump) = jump.as_ref() {
                            jump.register(
                                ui,
                                &response,
                                &format!("hotstring/{hotstring_identity:016x}/text"),
                                "Hotstring text",
                                TargetKind::TextEntry,
                            );
                        }
                        set_first_edit(
                            &mut continuous,
                            continuous_widget_edit(
                                &response,
                                format!("hotstring:{}.text", hotstring.id),
                            ),
                        );
                        let case_sensitive =
                            ui.checkbox(&mut hotstring.case_sensitive, "Case sensitive");
                        if let Some(jump) = jump.as_ref() {
                            jump.register(
                                ui,
                                &case_sensitive,
                                &format!("hotstring/{hotstring_identity:016x}/case-sensitive"),
                                "Hotstring case sensitivity",
                                TargetKind::Checkbox,
                            );
                        }
                        enum_combo(
                            ui,
                            "Scope",
                            &mut hotstring.scope,
                            &[
                                ("Menu local", crate::radial::model::TriggerScope::MenuLocal),
                                ("Global", crate::radial::model::TriggerScope::Global),
                            ],
                            ui_pass,
                        );
                        let remove = ui.button("Remove hotstring");
                        if let Some(jump) = jump.as_ref() {
                            jump.register(
                                ui,
                                &remove,
                                &format!("hotstring/{hotstring_identity:016x}/remove"),
                                "Remove hotstring",
                                TargetKind::Button,
                            );
                        }
                        if remove.clicked() {
                            remove_hotstring = Some(index);
                        }
                    },
                );
            }
            if let Some(index) = remove_hotstring {
                cell.hotstrings.remove(index);
            }
            let add_hotstring = ui.button("Add hotstring");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &add_hotstring,
                    "hotstring/add",
                    "Add hotstring",
                    TargetKind::Button,
                );
            }
            if add_hotstring.clicked() {
                cell.hotstrings.push(crate::radial::model::ItemHotstring {
                    id: session.allocate_hotstring_id("hotstring"),
                    text: ";radial".into(),
                    gesture: crate::radial::model::ClickGesture::Primary,
                    case_sensitive: false,
                    scope: crate::radial::model::TriggerScope::MenuLocal,
                });
            }
        },
    );
    continuous
}

fn document_rule_controls(
    ui: &mut egui::Ui,
    session: &mut RadialAuthoringSession,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
    ui_pass: &mut RadialUiPass,
) {
    let jump = ui_pass.jump.clone();
    let mut document = (*session.draft).clone();
    let original = document.clone();
    let mut continuous: Option<(String, ContinuousWidgetEdit)> = None;
    let menu_choices: Vec<_> = document
        .menus
        .iter()
        .map(|menu| (menu.id.clone(), menu.name.clone()))
        .collect();
    persisted_section(
        ui,
        "Context rules",
        "document:context-rules",
        expanded_sections,
        ui_pass.child_jump("sections"),
        |ui| {
            let mut remove = None;
            for (index, rule) in document.context_rules.iter_mut().enumerate() {
                let rule_digest = radial_value_digest(&rule.id);
                let rule_identity = format!("context-rule/{rule_digest:016x}");
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id(ui.id().with(("radial-context-rule-row", rule_digest))),
                    |ui| {
                        ui.group(|ui| {
                            let enabled = ui.checkbox(&mut rule.enabled, "Enabled");
                            if let Some(jump) = jump.as_ref() {
                                jump.register(
                                    ui,
                                    &enabled,
                                    &format!("{rule_identity}/enabled"),
                                    "Context rule enabled",
                                    TargetKind::Checkbox,
                                );
                            }
                            let priority_response = ui
                                .add(egui::DragValue::new(&mut rule.priority).prefix("Priority "));
                            if let Some(jump) = jump.as_ref() {
                                jump.register(
                                    ui,
                                    &priority_response,
                                    &format!("{rule_identity}/priority"),
                                    "Context rule priority",
                                    TargetKind::Slider,
                                );
                            }
                            set_first_edit(
                                &mut continuous,
                                continuous_widget_edit(&priority_response, "priority")
                                    .map(|edit| (rule_identity.clone(), edit)),
                            );
                            ui.label(format!("ID: {}", rule.id));
                            let process_edit = optional_text(
                                ui,
                                "Process",
                                &mut rule.process_name,
                                jump.as_ref(),
                                &rule_identity,
                                "process",
                            )
                            .map(|signals| ContinuousWidgetEdit {
                                field: "process_name".into(),
                                signals,
                            });
                            set_first_edit(
                                &mut continuous,
                                process_edit.map(|edit| (rule_identity.clone(), edit)),
                            );
                            let title_edit = optional_text(
                                ui,
                                "Window title contains",
                                &mut rule.window_title_contains,
                                jump.as_ref(),
                                &rule_identity,
                                "window-title",
                            )
                            .map(|signals| ContinuousWidgetEdit {
                                field: "window_title_contains".into(),
                                signals,
                            });
                            set_first_edit(
                                &mut continuous,
                                title_edit.map(|edit| (rule_identity.clone(), edit)),
                            );
                            let monitor_edit = optional_text(
                                ui,
                                "Monitor ID",
                                &mut rule.monitor_id,
                                jump.as_ref(),
                                &rule_identity,
                                "monitor",
                            )
                            .map(|signals| ContinuousWidgetEdit {
                                field: "monitor_id".into(),
                                signals,
                            });
                            set_first_edit(
                                &mut continuous,
                                monitor_edit.map(|edit| (rule_identity.clone(), edit)),
                            );
                            let combo_salt = ui.id().with(("context-rule-menu", rule_digest));
                            let combo_identity =
                                format!("combo/{:016x}", ui.make_persistent_id(combo_salt).value());
                            let combo_jump = jump
                                .as_ref()
                                .map(|pass| pass.child("combos").combo_popup(&combo_identity));
                            ui.label("Menu");
                            let menu_combo = egui::ComboBox::from_id_salt(combo_salt)
                                .selected_text(rule.menu_id.to_string())
                                .show_ui(ui, |ui| {
                                    for (id, name) in &menu_choices {
                                        let id_digest = radial_value_digest(id);
                                        let option = ui
                                            .scope_builder(
                                                egui::UiBuilder::new().id(ui.id().with((
                                                    "radial-context-rule-menu-option",
                                                    id_digest,
                                                ))),
                                                |ui| {
                                                    ui.selectable_value(
                                                        &mut rule.menu_id,
                                                        id.clone(),
                                                        name,
                                                    )
                                                },
                                            )
                                            .inner;
                                        register_radial_combo_option(
                                            combo_jump.as_ref(),
                                            ui,
                                            &option,
                                            &format!("{combo_identity}/option/{id_digest:016x}"),
                                            "Menu option",
                                        );
                                    }
                                });
                            register_radial_combo_anchor(
                                ui_pass,
                                &menu_combo.response,
                                &combo_identity,
                                "Context rule menu",
                            );
                            if let Some(jump) = jump.as_ref() {
                                jump.register(
                                    ui,
                                    &menu_combo.response,
                                    &combo_identity,
                                    "Context rule menu",
                                    TargetKind::ComboBox,
                                );
                            }
                            let remove_button = ui.button("Remove rule");
                            if let Some(jump) = jump.as_ref() {
                                jump.register(
                                    ui,
                                    &remove_button,
                                    &format!("{rule_identity}/remove"),
                                    "Remove context rule",
                                    TargetKind::Button,
                                );
                            }
                            if remove_button.clicked() {
                                remove = Some(index);
                            }
                        })
                    },
                );
            }
            if let Some(index) = remove {
                document.context_rules.remove(index);
            }
            let add_rule = ui.button("Add context rule");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &add_rule,
                    "context-rule/add",
                    "Add context rule",
                    TargetKind::Button,
                );
            }
            if add_rule.clicked()
                && let Some(menu) = document.menus.first()
            {
                let id = session.allocate_context_rule_id("context");
                let menu_id = menu.id.clone();
                document
                    .context_rules
                    .push(crate::radial::model::ContextRule {
                        id,
                        enabled: true,
                        priority: 0,
                        process_name: None,
                        window_title_contains: None,
                        monitor_id: None,
                        menu_id,
                    });
            }
        },
    );
    persisted_section(
        ui,
        "Custom triggers",
        "document:custom-triggers",
        expanded_sections,
        ui_pass.child_jump("sections"),
        |ui| {
            let mut remove = None;
            for (index, trigger) in document.custom_triggers.iter_mut().enumerate() {
                let trigger_digest = radial_value_digest(&trigger.id);
                let trigger_identity = format!("custom-trigger/{trigger_digest:016x}");
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .id(ui.id().with(("radial-custom-trigger-row", trigger_digest))),
                    |ui| {
                        ui.group(|ui| {
                            ui.label(format!("ID: {}", trigger.id));
                            let chord_response = ui.text_edit_singleline(&mut trigger.chord);
                            if let Some(jump) = jump.as_ref() {
                                jump.register(
                                    ui,
                                    &chord_response,
                                    &format!("{trigger_identity}/chord"),
                                    "Custom trigger chord",
                                    TargetKind::TextEntry,
                                );
                            }
                            set_first_edit(
                                &mut continuous,
                                continuous_widget_edit(&chord_response, "chord")
                                    .map(|edit| (trigger_identity.clone(), edit)),
                            );
                            enum_combo(
                                ui,
                                "Scope",
                                &mut trigger.scope,
                                &[
                                    ("Menu local", crate::radial::model::TriggerScope::MenuLocal),
                                    ("Global", crate::radial::model::TriggerScope::Global),
                                ],
                                ui_pass,
                            );
                            let combo_salt = ui.id().with(("custom-trigger-menu", trigger_digest));
                            let combo_identity =
                                format!("combo/{:016x}", ui.make_persistent_id(combo_salt).value());
                            let combo_jump = jump
                                .as_ref()
                                .map(|pass| pass.child("combos").combo_popup(&combo_identity));
                            ui.label("Menu");
                            let menu_combo = egui::ComboBox::from_id_salt(combo_salt)
                                .selected_text(trigger.menu_id.to_string())
                                .show_ui(ui, |ui| {
                                    for (id, name) in &menu_choices {
                                        let id_digest = radial_value_digest(id);
                                        let option = ui
                                            .scope_builder(
                                                egui::UiBuilder::new().id(ui.id().with((
                                                    "radial-custom-trigger-menu-option",
                                                    id_digest,
                                                ))),
                                                |ui| {
                                                    ui.selectable_value(
                                                        &mut trigger.menu_id,
                                                        id.clone(),
                                                        name,
                                                    )
                                                },
                                            )
                                            .inner;
                                        register_radial_combo_option(
                                            combo_jump.as_ref(),
                                            ui,
                                            &option,
                                            &format!("{combo_identity}/option/{id_digest:016x}"),
                                            "Menu option",
                                        );
                                    }
                                });
                            register_radial_combo_anchor(
                                ui_pass,
                                &menu_combo.response,
                                &combo_identity,
                                "Custom trigger menu",
                            );
                            if let Some(jump) = jump.as_ref() {
                                jump.register(
                                    ui,
                                    &menu_combo.response,
                                    &combo_identity,
                                    "Custom trigger menu",
                                    TargetKind::ComboBox,
                                );
                            }
                            let remove_button = ui.button("Remove trigger");
                            if let Some(jump) = jump.as_ref() {
                                jump.register(
                                    ui,
                                    &remove_button,
                                    &format!("{trigger_identity}/remove"),
                                    "Remove custom trigger",
                                    TargetKind::Button,
                                );
                            }
                            if remove_button.clicked() {
                                remove = Some(index);
                            }
                        })
                    },
                );
            }
            if let Some(index) = remove {
                document.custom_triggers.remove(index);
            }
            let add_trigger = ui.button("Add custom trigger");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &add_trigger,
                    "custom-trigger/add",
                    "Add custom trigger",
                    TargetKind::Button,
                );
            }
            if add_trigger.clicked()
                && let Some(menu) = document.menus.first()
            {
                let id = session.allocate_trigger_id("trigger");
                let chord = "Ctrl+Shift+1".into();
                let menu_id = menu.id.clone();
                document
                    .custom_triggers
                    .push(crate::radial::model::TriggerDefinition {
                        id,
                        chord,
                        menu_id,
                        scope: crate::radial::model::TriggerScope::MenuLocal,
                    });
            }
        },
    );
    persisted_section(
        ui,
        "Media search paths",
        "document:media-search-paths",
        expanded_sections,
        ui_pass.child_jump("sections"),
        |ui| {
            for (index, root) in document
                .media_search_roots
                .image_directories
                .iter_mut()
                .enumerate()
            {
                let response = ui.text_edit_singleline(root);
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &response,
                        &format!("media-search/image-root/{index}"),
                        "Image search root",
                        TargetKind::TextEntry,
                    );
                }
                set_first_edit(
                    &mut continuous,
                    continuous_widget_edit(&response, format!("image_directories[{index}]"))
                        .map(|edit| ("media-search-roots".into(), edit)),
                );
            }
            let add_image_root = ui.button("Add image search path");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &add_image_root,
                    "media-search/image-root/add",
                    "Add image search root",
                    TargetKind::Button,
                );
            }
            if add_image_root.clicked() {
                document
                    .media_search_roots
                    .image_directories
                    .push(String::new());
            }
            for (index, root) in document
                .media_search_roots
                .sound_directories
                .iter_mut()
                .enumerate()
            {
                let response = ui.text_edit_singleline(root);
                if let Some(jump) = jump.as_ref() {
                    jump.register(
                        ui,
                        &response,
                        &format!("media-search/sound-root/{index}"),
                        "Sound search root",
                        TargetKind::TextEntry,
                    );
                }
                set_first_edit(
                    &mut continuous,
                    continuous_widget_edit(&response, format!("sound_directories[{index}]"))
                        .map(|edit| ("media-search-roots".into(), edit)),
                );
            }
            let add_sound_root = ui.button("Add sound search path");
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &add_sound_root,
                    "media-search/sound-root/add",
                    "Add sound search root",
                    TargetKind::Button,
                );
            }
            if add_sound_root.clicked() {
                document
                    .media_search_roots
                    .sound_directories
                    .push(String::new());
            }
            let search_windows = ui.checkbox(
                &mut document.media_search_roots.search_windows_media_for_sounds,
                "Search Windows media for sounds",
            );
            if let Some(jump) = jump.as_ref() {
                jump.register(
                    ui,
                    &search_windows,
                    "media-search/windows-media",
                    "Search Windows media",
                    TargetKind::Checkbox,
                );
            }
        },
    );
    if document != original || continuous.is_some() {
        if let Some((entity, edit)) = continuous {
            let _ =
                dispatch_widget_document_edit(session, document, entity, &edit.field, edit.signals);
        } else {
            let _ = session.replace_document_atomic(document);
        }
    }
}

fn optional_text(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<String>,
    jump: Option<&RadialJumpPass>,
    identity: &str,
    field: &str,
) -> Option<WidgetEditSignals> {
    let mut enabled = value.is_some();
    let mut signals = None;
    ui.horizontal(|ui| {
        let enabled_response = ui.checkbox(&mut enabled, label);
        if let Some(jump) = jump {
            jump.register(
                ui,
                &enabled_response,
                &format!("{identity}/{field}/enabled"),
                label,
                TargetKind::Checkbox,
            );
        }
        if enabled_response.changed() {
            *value = enabled.then(String::new);
        }
        if let Some(value) = value.as_mut() {
            let response = ui.text_edit_singleline(value);
            if let Some(jump) = jump {
                jump.register(
                    ui,
                    &response,
                    &format!("{identity}/{field}"),
                    label,
                    TargetKind::TextEntry,
                );
            }
            if response.changed() || response.lost_focus() || response.drag_stopped() {
                signals = Some(WidgetEditSignals::from_response(&response));
            }
        }
    });
    signals
}

fn relocate_and_delete_ring(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    source_ring: &RingId,
    destination_ring: &RingId,
) -> Result<(), String> {
    if source_ring == destination_ring {
        return Err("source and destination ring are identical".into());
    }
    let mut document = (*session.draft).clone();
    let menu = document
        .menus
        .iter_mut()
        .find(|menu| &menu.id == menu_id)
        .ok_or("menu missing")?;
    if menu.rings.len() <= 1 {
        return Err("cannot delete the last ring".into());
    }
    let source_index = menu
        .rings
        .iter()
        .position(|ring| &ring.id == source_ring)
        .ok_or("source ring missing")?;
    let cells = menu.rings[source_index].cells.clone();
    let destination = menu
        .rings
        .iter_mut()
        .find(|ring| &ring.id == destination_ring)
        .ok_or("destination ring missing")?;
    destination.cells.extend(cells);
    menu.rings.remove(source_index);
    session
        .replace_document_atomic(document)
        .map_err(|error| format!("{error:?}"))
}

#[derive(Clone, Copy)]
enum MenuActionSlot {
    CenterPrimary,
    CenterSecondary,
    BackgroundPrimary,
    BackgroundSecondary,
}

fn assign_menu_action(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    slot: MenuActionSlot,
    binding: crate::radial::model::ActionBinding,
) -> Result<(), AuthoringError> {
    let mut document = (*session.draft).clone();
    let menu = document
        .menus
        .iter_mut()
        .find(|menu| &menu.id == menu_id)
        .ok_or(AuthoringError::MissingEntity)?;
    match slot {
        MenuActionSlot::CenterPrimary => {
            menu.center_action = Some(binding);
            menu.center_control = None;
        }
        MenuActionSlot::CenterSecondary => {
            menu.center_secondary_action = Some(binding);
            menu.center_secondary_control = None;
        }
        MenuActionSlot::BackgroundPrimary => {
            menu.background_action = Some(binding);
            menu.background_control = None;
        }
        MenuActionSlot::BackgroundSecondary => {
            menu.background_secondary_action = Some(binding);
            menu.background_secondary_control = None;
        }
    }
    session.replace_document_atomic(document)
}

fn assign_alternate_click(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    ring_id: &RingId,
    cell_id: &CellId,
    gesture: crate::radial::model::ClickGesture,
    action: crate::radial::model::ActionBinding,
) -> Result<(), AuthoringError> {
    let mut document = (*session.draft).clone();
    let cell = document
        .menus
        .iter_mut()
        .find(|menu| &menu.id == menu_id)
        .and_then(|menu| menu.rings.iter_mut().find(|ring| &ring.id == ring_id))
        .and_then(|ring| ring.cells.iter_mut().find(|cell| &cell.id == cell_id))
        .ok_or(AuthoringError::MissingEntity)?;
    cell.alternate_controls
        .retain(|entry| entry.gesture != gesture);
    if let Some(existing) = cell
        .alternate_clicks
        .iter_mut()
        .find(|entry| entry.gesture == gesture)
    {
        existing.action = action;
    } else {
        cell.alternate_clicks
            .push(crate::radial::model::ClickBinding {
                gesture,
                action,
                after_action: AfterActionPolicy::Inherit,
            });
    }
    session.replace_document_atomic(document)
}

fn report_action_assignment_result(
    session: &mut RadialAuthoringSession,
    result: Result<(), AuthoringError>,
) {
    if let Err(error) = result {
        session.last_error = Some(format!("Action assignment failed: {error:?}"));
    }
}

fn preview_legacy_files(
    paths: &[std::path::PathBuf],
    source: crate::radial::compatibility::CompatibilitySource,
    session: &RadialAuthoringSession,
) -> Result<PendingImport, String> {
    let mut owned = Vec::new();
    let mut total = 0usize;
    let common_parent = paths.first().and_then(|path| path.parent());
    for path in paths {
        let bytes =
            asset_picker::read_bounded(path, crate::radial::model::limits::MAX_IMPORT_BYTES)?;
        total = total
            .checked_add(bytes.len())
            .ok_or("legacy input byte count overflow")?;
        if total > crate::radial::model::limits::MAX_IMPORT_BYTES as usize {
            return Err("legacy inputs exceed the import byte budget".into());
        }
        let relative = common_parent
            .and_then(|parent| path.strip_prefix(parent).ok())
            .unwrap_or(path.as_path());
        let name = relative
            .to_str()
            .ok_or("legacy input has a non-Unicode file name")?
            .replace('\\', "/");
        owned.push((name, bytes));
    }
    let inputs = owned
        .iter()
        .map(|(name, bytes)| crate::radial::import::ImportInput {
            relative_path: name,
            bytes,
            evidence: crate::radial::import::ImportEvidence::UserSelected,
        })
        .collect::<Vec<_>>();
    let menu_ids = session
        .draft
        .menus
        .iter()
        .map(|menu| menu.id.as_str().to_owned())
        .collect();
    let skin_ids = session
        .draft
        .skins
        .iter()
        .map(|skin| skin.id.as_str().to_owned())
        .collect();
    let preview = match source {
        crate::radial::compatibility::CompatibilitySource::Radify => {
            crate::radial::import::preview_radify(&inputs, "Imported Radify", &menu_ids, &skin_ids)
        }
        crate::radial::compatibility::CompatibilitySource::RadialMenuV4 => {
            crate::radial::import::preview_rm4(&inputs, "Imported RM4", &menu_ids, &skin_ids)
        }
    };
    Ok(PendingImport::legacy(preview, session))
}

fn open_designer_file_dialog(request: &DesignerFileDialogRequest) -> DesignerFileDialogResult {
    match request {
        DesignerFileDialogRequest::StyleMedia { .. }
        | DesignerFileDialogRequest::BulkStyleMedia { .. }
        | DesignerFileDialogRequest::CellExternalIcon { .. } => {
            rfd::FileDialog::new().pick_file().map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            )
        }
        DesignerFileDialogRequest::ImportManaged {
            label, extensions, ..
        } => {
            let extensions = extensions.iter().map(String::as_str).collect::<Vec<_>>();
            rfd::FileDialog::new()
                .add_filter(label, &extensions)
                .pick_file()
                .map_or(
                    DesignerFileDialogResult::Cancelled,
                    DesignerFileDialogResult::File,
                )
        }
        DesignerFileDialogRequest::PreviewPackage => rfd::FileDialog::new()
            .add_filter("Multi Launcher radial", &["mlradial"])
            .pick_file()
            .map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            ),
        DesignerFileDialogRequest::ExportMenu { .. } => rfd::FileDialog::new()
            .add_filter("Multi Launcher radial", &["mlradial"])
            .set_file_name("menu.mlradial")
            .save_file()
            .map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            ),
        DesignerFileDialogRequest::ExportPackage { .. } => rfd::FileDialog::new()
            .add_filter("Multi Launcher radial", &["mlradial"])
            .set_file_name("all-radial-menus.mlradial")
            .save_file()
            .map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            ),
        DesignerFileDialogRequest::ExportSkin { .. } => rfd::FileDialog::new()
            .add_filter("Multi Launcher radial", &["mlradial"])
            .set_file_name("skin.mlradial")
            .save_file()
            .map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            ),
        DesignerFileDialogRequest::PreviewLegacy { .. } => {
            rfd::FileDialog::new().pick_files().map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::Files,
            )
        }
        DesignerFileDialogRequest::ReplaceBackup => rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("radial-before-import.json")
            .save_file()
            .map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            ),
    }
}

fn acceptance_observation_digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn acceptance_authored_target_digest(target: &crate::radial::authoring::AuthoredCellTarget) -> u64 {
    acceptance_identity_digest(&[
        target.menu_id.as_str(),
        target.ring_id.as_str(),
        target.cell_id.as_str(),
    ])
}

fn acceptance_identity_digest(parts: &[&str]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for byte in part.as_bytes() {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn acceptance_staged_properties_digest(
    session_id: u64,
    target_digest: u64,
    draft: &PropertiesDraft,
) -> Result<u64, String> {
    let fields = serde_json::to_vec(&(
        session_id,
        draft.generation.0,
        &draft.label,
        draft.content_kind,
        &draft.action_binding,
        draft.action_editor.authored_input_digest(),
        draft.dynamic_source,
        &draft.submenu_target,
        draft.icon_kind,
        &draft.original_content,
        &draft.original_icon,
    ))
    .map_err(|error| format!("could not summarize staged Properties fields: {error}"))?;
    let mut bytes = Vec::with_capacity(std::mem::size_of::<u64>() + fields.len());
    bytes.extend_from_slice(&target_digest.to_le_bytes());
    bytes.extend_from_slice(&fields);
    Ok(acceptance_observation_digest(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn radial_viewport_test_app(
        ctx: &egui::Context,
        root: &std::path::Path,
    ) -> crate::gui::LauncherApp {
        let mut settings = crate::settings::Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        settings.dashboard.enabled = false;

        let mut app = crate::gui::LauncherApp::new(
            ctx,
            std::sync::Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        app.dashboard_initial_refresh_queued = true;
        app
    }

    fn production_source(source: &str) -> String {
        let normalized = source.replace("\r\n", "\n");
        normalized
            .split_once("\n#[cfg(test)]\nmod tests")
            .expect("radial editor source must contain the test-module boundary")
            .0
            .to_owned()
    }

    #[test]
    fn production_source_excludes_tests_with_lf_and_crlf() {
        let source = "production\n#[cfg(test)]\nmod tests { forbidden_test_literal }";
        for source in [source.to_owned(), source.replace("\n", "\r\n")] {
            assert_eq!(production_source(&source), "production");
        }
    }

    fn retained_tree_search_undo_scenario(settle_starter: bool) {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.tree_visible = true;
        editor.session.as_mut().unwrap().font_catalog_loaded = true;
        editor.inspector_visible = false;
        let session = editor.session.as_mut().unwrap();
        let menu_id = session.draft.menus[0].id.clone();
        menu::rename_menu(
            session,
            menu_id.clone(),
            "Unsaved local undo fixture".into(),
            EditPhase::Atomic,
        )
        .unwrap();
        session.select(Some(StableSelection::Menu(menu_id)));
        assert!(session.is_dirty());
        assert_eq!(session.acceptance_history_depths(), (1, 0));
        let before = (
            session.draft.clone(),
            session.editor_session,
            session.generation,
            session.selection.clone(),
            session.acceptance_history_depths(),
            session.pending_assets.clone(),
        );
        let driver = RetainedDesignerDriver::new(editor);
        let context = driver.context;
        let frame_context = driver.frame;
        let mut editor = driver.editor;
        let viewport_id = radial_designer_viewport_id();
        let frame = |editor: &mut RadialEditorState,
                     time: f64,
                     events: Vec<egui::Event>,
                     modifiers: egui::Modifiers| {
            let client_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(520.0, 380.0));
            let mut events = events;
            events.push(egui::Event::ModifiersChanged(modifiers));
            let mut input = egui::RawInput {
                viewport_id,
                screen_rect: Some(client_rect),
                time: Some(time),
                focused: true,
                events,
                ..Default::default()
            };
            input
                .viewports
                .insert(egui::ViewportId::ROOT, egui::ViewportInfo::default());
            input.viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    focused: Some(true),
                    inner_rect: Some(client_rect),
                    ..Default::default()
                },
            );
            context.run_ui(input, |ctx| {
                editor.viewport_ui(ctx, &frame_context, egui::ViewportClass::Deferred);
            })
        };
        let output = frame(&mut editor, 0.0, Vec::new(), egui::Modifiers::default());
        let point = accesskit_value_bounds(&output, egui::accesskit::Role::TextInput, "").center();
        output.drop_without_applying_deltas();
        frame(
            &mut editor,
            0.01,
            vec![egui::Event::PointerMoved(point)],
            egui::Modifiers::default(),
        )
        .drop_without_applying_deltas();
        frame(
            &mut editor,
            0.02,
            vec![pointer_button_event(point, true)],
            egui::Modifiers::default(),
        )
        .drop_without_applying_deltas();
        frame(
            &mut editor,
            0.03,
            vec![pointer_button_event(point, false)],
            egui::Modifiers::default(),
        )
        .drop_without_applying_deltas();
        let field_id = context
            .memory(|memory| memory.focused())
            .expect("actual TreeSearch has focus");
        assert!(
            designer_tree_search_undo_scope(&context, egui::Id::new("absent field"), "starter")
                .is_none()
        );
        let scope = |editor: &RadialEditorState, in_flux| {
            assert_eq!(context.memory(|memory| memory.focused()), Some(field_id));
            assert_eq!(
                designer_tree_search_undo_scope(&context, field_id, &editor.tree_filter),
                Some(
                    acceptance_trace::DesignerAuthoringControlScope::TreeSearchUndo {
                        field_id_digest: field_id.value(),
                        value_digest: acceptance_observation_digest(editor.tree_filter.as_bytes()),
                        in_flux,
                    }
                )
            );
        };
        scope(&editor, false);
        frame(
            &mut editor,
            0.1,
            vec![egui::Event::Text("starter".into())],
            egui::Modifiers::default(),
        )
        .drop_without_applying_deltas();
        assert_eq!(editor.tree_filter, "starter");
        scope(&editor, true);
        let replacement_time = if settle_starter {
            frame(&mut editor, 1.2, Vec::new(), egui::Modifiers::default())
                .drop_without_applying_deltas();
            assert_eq!(editor.tree_filter, "starter");
            scope(&editor, false);
            1.3
        } else {
            0.2
        };
        let control = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        frame(
            &mut editor,
            replacement_time,
            vec![
                key_event(egui::Key::A, control, true),
                key_event(egui::Key::A, control, false),
            ],
            control,
        )
        .drop_without_applying_deltas();
        frame(
            &mut editor,
            replacement_time + 0.05,
            vec![egui::Event::Text("replacement authored label".into())],
            egui::Modifiers::default(),
        )
        .drop_without_applying_deltas();
        assert_eq!(editor.tree_filter, "replacement authored label");
        scope(&editor, true);
        frame(
            &mut editor,
            replacement_time + 0.1,
            vec![
                key_event(egui::Key::Z, control, true),
                key_event(egui::Key::Z, control, false),
            ],
            control,
        )
        .drop_without_applying_deltas();
        assert_eq!(
            editor.tree_filter,
            if settle_starter { "starter" } else { "" }
        );
        scope(&editor, false);
        let session = editor.session.as_ref().unwrap();
        assert!(session.is_dirty());
        assert_eq!(
            before,
            (
                session.draft.clone(),
                session.editor_session,
                session.generation,
                session.selection.clone(),
                session.acceptance_history_depths(),
                session.pending_assets.clone(),
            )
        );
    }

    #[test]
    fn actual_tree_search_rapid_edits_coalesce_to_the_seeded_empty_checkpoint() {
        retained_tree_search_undo_scenario(false);
    }

    #[test]
    fn actual_tree_search_settled_starter_checkpoint_restores_starter_on_ctrl_z() {
        retained_tree_search_undo_scenario(true);
    }

    #[test]
    fn designer_viewport_matches_launcher_topmost_policy() {
        assert_eq!(
            designer_viewport_builder(true, true).window_level,
            Some(egui::viewport::WindowLevel::AlwaysOnTop)
        );
        assert_eq!(designer_viewport_builder(true, false).window_level, None);
    }

    #[test]
    fn bulk_edit_buffers_are_bound_to_exact_selection_and_generation() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_ref().expect("test session");
        let menu = &session.draft.menus[0];
        let ring = &menu.rings[0];
        let target = crate::radial::authoring::AuthoredCellTarget {
            menu_id: menu.id.clone(),
            ring_id: ring.id.clone(),
            cell_id: ring.cells[0].id.clone(),
        };
        let other_target = crate::radial::authoring::AuthoredCellTarget {
            cell_id: ring.cells[1].id.clone(),
            ..target.clone()
        };
        let scope = BulkEditStageScope {
            editor_session: session.editor_session,
            generation: session.generation,
            targets: vec![target.clone()],
        };

        editor.reconcile_bulk_edit_stage(Some(scope.clone()));
        editor.bulk_label_value = "staged for first cell".into();
        editor.bulk_style_text = "staged media".into();
        editor.bulk_style_staged_value = Some(serde_json::json!(true));
        editor.bulk_style_mode = BulkStyleMode::Clear;
        editor.bulk_after_action_policy = AfterActionPolicy::CloseTree;

        editor.reconcile_bulk_edit_stage(Some(BulkEditStageScope {
            targets: vec![other_target],
            ..scope.clone()
        }));
        assert!(editor.bulk_label_value.is_empty());
        assert!(editor.bulk_style_text.is_empty());
        assert_eq!(editor.bulk_style_mode, BulkStyleMode::Set);
        assert!(editor.bulk_style_staged_value.is_none());
        assert_eq!(editor.bulk_after_action_policy, AfterActionPolicy::Inherit);

        editor.bulk_label_value = "staged at old generation".into();
        editor.reconcile_bulk_edit_stage(Some(BulkEditStageScope {
            generation: crate::radial::authoring::DraftGeneration(scope.generation.0 + 1),
            ..scope
        }));
        assert!(editor.bulk_label_value.is_empty());
    }

    #[test]
    fn search_reveal_reopens_a_retained_collapsed_tree_header_and_restores_focus() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_ref().expect("test session");
        let menu_id = session.draft.default_menu_id.clone();
        let menu = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .expect("default menu");
        let target = StableSelection::Cell {
            menu_id: menu_id.clone(),
            ring_id: menu.rings[0].id.clone(),
            cell_id: menu.rings[0].cells[0].id.clone(),
        };
        let expansion_key = format!("menu:{menu_id}");
        editor
            .preferences
            .expanded_sections
            .insert(expansion_key, false);

        let context = egui::Context::default();
        let defaults = crate::radial::model::RadialFeatureSettings::default();
        let rendered_header_id = std::cell::Cell::new(None);
        let render_tree = |editor: &mut RadialEditorState| {
            let is_open = std::cell::Cell::new(false);
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(720.0, 640.0),
                        )),
                        ..Default::default()
                    },
                    |root_ui| {
                        egui::CentralPanel::default().show(root_ui, |ui| {
                            let header_id = ui.make_persistent_id(egui::Id::new((
                                editor.tree_widget_epoch,
                                menu::widget_key("menu", menu_id.as_str(), "tree"),
                            )));
                            let default_open = editor
                                .preferences
                                .expanded_sections
                                .get(&format!("menu:{menu_id}"))
                                .copied()
                                .unwrap_or(false);
                            rendered_header_id.set(Some(header_id));
                            editor.tree(
                                ui,
                                &defaults,
                                ViewportClass::Deferred,
                                &RadialUiPass::default(),
                            );
                            is_open.set(
                                egui::collapsing_header::CollapsingState::load_with_default_open(
                                    ui.ctx(),
                                    header_id,
                                    default_open,
                                )
                                .is_open(),
                            );
                        });
                    },
                )
                .drop_without_applying_deltas();
            is_open.get()
        };

        assert!(!render_tree(&mut editor));
        let collapsed_header_id = rendered_header_id.get().expect("rendered header ID");
        let mut retained_collapsed =
            egui::collapsing_header::CollapsingState::load_with_default_open(
                &context,
                collapsed_header_id,
                false,
            );
        retained_collapsed.set_open(true);
        retained_collapsed.store(&context);
        assert!(render_tree(&mut editor));
        retained_collapsed.set_open(false);
        retained_collapsed.store(&context);
        assert!(!render_tree(&mut editor));
        assert!(
            !egui::collapsing_header::CollapsingState::load(&context, collapsed_header_id)
                .expect("test installed a retained collapsed state")
                .is_open()
        );

        let previous_epoch = editor.tree_widget_epoch;
        editor.apply_navigation_intent(DesignerNavigationIntent::Select {
            target: target.clone(),
            control: false,
            shift: false,
            force_history: true,
            open_properties: false,
        });
        assert_eq!(editor.tree_widget_epoch, previous_epoch.wrapping_add(1));
        assert!(editor.preferences.expanded_sections[&format!("menu:{menu_id}")]);
        assert!(render_tree(&mut editor));
        let revealed_header_id = rendered_header_id.get().expect("re-rendered header ID");
        assert_ne!(revealed_header_id, collapsed_header_id);
        assert!(
            egui::collapsing_header::CollapsingState::load_with_default_open(
                &context,
                revealed_header_id,
                true,
            )
            .is_open()
        );
        assert_eq!(editor.focus_restore, None);
    }

    #[test]
    fn tree_search_navigation_guard_supports_cancel_discard_and_apply() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.tree_visible = true;
        editor.tree_width = 300.0;
        editor.inspector_visible = true;
        editor.inspector_width = 320.0;
        editor.tree_filter = "Screen Draw".into();
        editor
            .preferences
            .expanded_sections
            .insert("menu:starter".into(), true);
        let (target, scope) = {
            let session = editor.session.as_mut().expect("test session");
            let menu = session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == MenuId::new("starter"))
                .expect("root menu");
            let target = StableSelection::Cell {
                menu_id: menu.id.clone(),
                ring_id: menu.rings[0].id.clone(),
                cell_id: menu.rings[0].cells[0].id.clone(),
            };
            session.select(Some(target.clone()));
            let scope = BindingEditorScope {
                surface: BindingEditorSurface::Inspector,
                editor_session: session.editor_session,
                draft_generation: session.generation,
                target: target.clone(),
                slot: BindingEditorSlot::CellPrimary,
            };
            (target, scope)
        };
        editor.inspector_action_editor_scope = Some(scope.clone());
        editor
            .inspector_action_editor
            .seed_unassigned_query_for_test(scope, None, "unassigned query");
        let mut driver = RetainedDesignerDriver::new(editor);

        let guarded = click_tree_search_result(&mut driver, "Screen Draw");
        let session = driver.editor.session.as_ref().expect("authoring session");
        let destination = authored_search_hits(&session.draft, "Screen Draw")
            .into_iter()
            .find_map(|hit| match hit.target {
                target @ StableSelection::Cell { .. } => Some(target),
                _ => None,
            })
            .expect("Screen Draw cell search result");
        assert_eq!(session.selection.as_ref(), Some(&target));
        assert!(driver.editor.pending_scope_change.is_some());
        assert!(driver.editor.inspector_action_editor.has_unassigned_text());
        assert!(
            accesskit_named_bounds(&guarded, "Cancel change", egui::accesskit::Role::Button,)
                .is_positive()
        );
        let identity_before_cancel = driver
            .editor
            .inspector_action_editor
            .identity()
            .expect("active Inspector identity");

        let cancel =
            accesskit_named_bounds(&guarded, "Cancel change", egui::accesskit::Role::Button)
                .center();
        guarded.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(cancel)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(cancel, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(cancel, false)])
            .drop_without_applying_deltas();

        assert!(driver.editor.pending_scope_change.is_none());
        assert!(driver.editor.inspector_action_editor.has_unassigned_text());
        assert_eq!(
            driver.editor.inspector_action_editor.identity(),
            Some(identity_before_cancel)
        );
        assert_eq!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&target)
        );

        let guarded = click_tree_search_result(&mut driver, "Screen Draw");
        let discard = accesskit_named_bounds(
            &guarded,
            "Discard and continue",
            egui::accesskit::Role::Button,
        )
        .center();
        guarded.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(discard)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(discard, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(discard, false)])
            .drop_without_applying_deltas();
        assert_eq!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&destination)
        );
        assert!(!driver.editor.inspector_action_editor.has_unassigned_text());

        let (generation, editor_session) = {
            let session = driver.editor.session.as_mut().unwrap();
            session.select(Some(target.clone()));
            (session.generation, session.editor_session)
        };
        let scope = BindingEditorScope {
            surface: BindingEditorSurface::Inspector,
            editor_session,
            draft_generation: generation,
            target: target.clone(),
            slot: BindingEditorSlot::CellPrimary,
        };
        driver.editor.inspector_action_editor_scope = Some(scope.clone());
        driver
            .editor
            .inspector_action_editor
            .seed_unassigned_query_for_test(scope, None, "apply this action");

        let guarded = click_tree_search_result(&mut driver, "Screen Draw");
        let apply = accesskit_named_bounds(
            &guarded,
            "Apply and continue",
            egui::accesskit::Role::Button,
        )
        .center();
        guarded.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(apply)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(apply, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(apply, false)])
            .drop_without_applying_deltas();
        let session = driver.editor.session.as_ref().unwrap();
        assert_eq!(session.selection.as_ref(), Some(&destination));
        let StableSelection::Cell {
            menu_id,
            ring_id,
            cell_id,
        } = &target
        else {
            unreachable!();
        };
        let cell = session
            .draft
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
            .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == cell_id))
            .expect("source cell remains authored");
        assert!(matches!(
            &cell.content,
            CellContent::Action {
                binding: ActionBinding::LauncherQuery { query, .. }
            } if query == "apply this action"
        ));
    }

    #[test]
    fn properties_query_must_be_saved_before_navigation_apply_and_continue() {
        let (mut driver, target, initial) = retained_properties_popup();
        let typed = replace_retained_text(&mut driver, &initial, "", "saved only on request");
        initial.drop_without_applying_deltas();
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .query_for_test(),
            "saved only on request"
        );
        let popup_apply =
            accesskit_named_bounds(&typed, "Apply", egui::accesskit::Role::Button).center();
        typed.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(popup_apply)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(popup_apply, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(popup_apply, false)])
            .drop_without_applying_deltas();
        assert_eq!(driver.editor.properties_popup.as_ref(), Some(&target));
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .query_for_test(),
            "saved only on request",
            "Properties Apply cannot close over pending action text"
        );

        queue_navigation_to_screenshot_region(&mut driver);
        let guarded = driver.frame(Vec::new());
        assert!(driver.editor.pending_scope_change.is_some());
        assert_eq!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&target)
        );
        let apply = accesskit_named_bounds(
            &guarded,
            "Apply and continue",
            egui::accesskit::Role::Button,
        )
        .center();
        guarded.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(apply)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(apply, true)])
            .drop_without_applying_deltas();
        let still_guarded = driver.frame(vec![pointer_button_event(apply, false)]);
        assert!(driver.editor.pending_scope_change.is_some());
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .query_for_test(),
            "saved only on request",
            "Apply and continue cannot erase unsaved Properties text"
        );

        let cancelled = click_retained_button(&mut driver, &still_guarded, "Cancel navigation");
        still_guarded.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        assert_eq!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&target)
        );
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .query_for_test(),
            "saved only on request",
            "Cancel navigation keeps the Properties buffer"
        );
        assert!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .has_unassigned_text()
        );

        let saved = click_retained_button(&mut driver, &cancelled, "Save query");
        cancelled.drop_without_applying_deltas();
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_binding,
            Some(ActionBinding::LauncherQuery {
                query: "saved only on request".into(),
                mode: crate::radial::model::QueryRunMode::OpenLauncher,
            })
        );
        assert!(
            !driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .has_unassigned_text()
        );
        saved.drop_without_applying_deltas();

        queue_navigation_to_screenshot_region(&mut driver);
        let guarded = driver.frame(Vec::new());
        let apply = accesskit_named_bounds(
            &guarded,
            "Apply and continue",
            egui::accesskit::Role::Button,
        )
        .center();
        guarded.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(apply)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(apply, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(apply, false)])
            .drop_without_applying_deltas();
        let session = driver.editor.session.as_ref().unwrap();
        assert_ne!(session.selection.as_ref(), Some(&target));
        let StableSelection::Cell {
            menu_id,
            ring_id,
            cell_id,
        } = &target
        else {
            unreachable!();
        };
        let cell = session
            .draft
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
            .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == cell_id))
            .expect("Properties target remains in the draft");
        assert!(matches!(
            &cell.content,
            CellContent::Action {
                binding: ActionBinding::LauncherQuery { query, .. }
            } if query == "saved only on request"
        ));
    }

    #[test]
    fn properties_command_must_be_set_before_navigation_apply_and_continue() {
        let (mut driver, target, initial) = retained_properties_popup();
        let advanced = click_retained_named_control(&mut driver, &initial, "Advanced");
        initial.drop_without_applying_deltas();
        assert!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .advanced_for_test()
        );
        let command_bounds = accesskit_value_bounds_by_vertical_order(
            &advanced,
            egui::accesskit::Role::TextInput,
            "",
            0,
        );
        replace_retained_text_at(&mut driver, command_bounds.center(), "window:close")
            .drop_without_applying_deltas();
        advanced.drop_without_applying_deltas();
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .command_for_test(),
            "window:close"
        );

        queue_navigation_to_screenshot_region(&mut driver);
        let guarded = driver.frame(Vec::new());
        let apply = accesskit_named_bounds(
            &guarded,
            "Apply and continue",
            egui::accesskit::Role::Button,
        )
        .center();
        guarded.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(apply)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(apply, true)])
            .drop_without_applying_deltas();
        let still_guarded = driver.frame(vec![pointer_button_event(apply, false)]);
        assert!(driver.editor.pending_scope_change.is_some());
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .command_for_test(),
            "window:close"
        );

        let cancelled = click_retained_button(&mut driver, &still_guarded, "Cancel navigation");
        still_guarded.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        assert_eq!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&target)
        );
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .command_for_test(),
            "window:close"
        );

        let set = click_retained_button(&mut driver, &cancelled, "Use exact command");
        cancelled.drop_without_applying_deltas();
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_binding,
            Some(ActionBinding::ExactCommand {
                command: "window:close".into(),
                args: None,
            })
        );
        set.drop_without_applying_deltas();
        queue_navigation_to_screenshot_region(&mut driver);
        let guarded = driver.frame(Vec::new());
        let apply = accesskit_named_bounds(
            &guarded,
            "Apply and continue",
            egui::accesskit::Role::Button,
        )
        .center();
        guarded.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(apply)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(apply, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(apply, false)])
            .drop_without_applying_deltas();
        let session = driver.editor.session.as_ref().unwrap();
        assert_ne!(session.selection.as_ref(), Some(&target));
        let StableSelection::Cell {
            menu_id,
            ring_id,
            cell_id,
        } = &target
        else {
            unreachable!();
        };
        let cell = session
            .draft
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
            .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == cell_id))
            .expect("Properties target remains in the draft");
        assert!(matches!(
            &cell.content,
            CellContent::Action {
                binding: ActionBinding::ExactCommand { command, args }
            } if command == "window:close" && args.is_none()
        ));
    }

    #[test]
    fn properties_action_text_is_discarded_only_by_explicit_discard() {
        let (mut driver, target, initial) = retained_properties_popup();
        replace_retained_text(&mut driver, &initial, "", "discard this draft")
            .drop_without_applying_deltas();
        initial.drop_without_applying_deltas();
        queue_navigation_to_screenshot_region(&mut driver);
        let guarded = driver.frame(Vec::new());
        let discarded = click_retained_button(&mut driver, &guarded, "Discard and continue");
        guarded.drop_without_applying_deltas();

        assert!(driver.editor.pending_scope_change.is_none());
        assert!(driver.editor.properties_draft.is_none());
        assert!(driver.editor.properties_popup.is_none());
        assert_ne!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&target)
        );
        let (menu_id, ring_id, cell_id) = cell_target_parts(&target);
        assert!(matches!(
            &authored_cell(&driver.editor, &menu_id, &ring_id, &cell_id).content,
            CellContent::Action {
                binding: ActionBinding::Persisted { .. }
            }
        ));
        discarded.drop_without_applying_deltas();
    }

    #[test]
    fn pinned_search_text_does_not_guard_navigation() {
        let (mut driver, target, initial) = retained_properties_popup();
        let pin = test_pinned_binding();
        let search_query = "find help before pinning";
        seed_pinned_properties_draft(&mut driver, &target, pin.clone(), search_query);
        initial.drop_without_applying_deltas();
        driver.frame(Vec::new()).drop_without_applying_deltas();
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .query_for_test(),
            search_query
        );
        queue_navigation_to_screenshot_region(&mut driver);
        driver.frame(Vec::new()).drop_without_applying_deltas();
        assert!(
            driver.editor.pending_scope_change.is_none(),
            "a Pin's retained search text stays outside the dirty guard"
        );
        assert_ne!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&target)
        );
        let (menu_id, ring_id, cell_id) = cell_target_parts(&target);
        assert_eq!(
            authored_cell(&driver.editor, &menu_id, &ring_id, &cell_id).content,
            CellContent::Action { binding: pin }
        );
    }

    #[test]
    fn editing_query_after_pin_restores_the_dirty_navigation_guard() {
        let (mut driver, target, initial) = retained_properties_popup();
        let pin = test_pinned_binding();
        let search_query = "find help before pinning";
        seed_pinned_properties_draft(&mut driver, &target, pin.clone(), search_query);
        initial.drop_without_applying_deltas();
        let current = driver.frame(Vec::new());
        let edited = replace_retained_text(
            &mut driver,
            &current,
            search_query,
            "query authored after pin",
        );
        current.drop_without_applying_deltas();
        edited.drop_without_applying_deltas();
        assert!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .has_unassigned_text()
        );

        queue_navigation_to_screenshot_region(&mut driver);
        let guarded = driver.frame(Vec::new());
        assert!(driver.editor.pending_scope_change.is_some());
        let apply = accesskit_named_bounds(
            &guarded,
            "Apply and continue",
            egui::accesskit::Role::Button,
        )
        .center();
        guarded.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(apply)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(apply, true)])
            .drop_without_applying_deltas();
        let still_guarded = driver.frame(vec![pointer_button_event(apply, false)]);
        assert!(driver.editor.pending_scope_change.is_some());
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .query_for_test(),
            "query authored after pin"
        );
        let cancelled = click_retained_button(&mut driver, &still_guarded, "Cancel navigation");
        cancelled.drop_without_applying_deltas();
        still_guarded.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        assert_eq!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&target)
        );
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_binding,
            Some(pin)
        );
    }

    #[test]
    fn properties_pin_control_stages_exact_binding_then_apply_continues_navigation() {
        let (mut driver, target, initial) = retained_properties_popup();
        let pin = test_pinned_binding();
        seed_properties_search_result(&mut driver, &target, pin.clone(), "find a result to pin");
        initial.drop_without_applying_deltas();
        let shown = driver.frame(Vec::new());
        let selected = click_retained_named_fragment(
            &mut driver,
            &shown,
            "Fixture action",
            egui::accesskit::Role::Button,
        );
        shown.drop_without_applying_deltas();
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .selected_binding(),
            Some(pin.clone())
        );
        let pinned = click_retained_button(&mut driver, &selected, "Pin this action");
        selected.drop_without_applying_deltas();
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_binding,
            Some(pin.clone()),
            "the real Pin control stages its exact row binding"
        );
        assert!(
            !driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .has_unassigned_text()
        );

        queue_navigation_to_screenshot_region(&mut driver);
        let guarded = driver.frame(Vec::new());
        assert!(driver.editor.pending_scope_change.is_some());
        let continued = click_retained_button(&mut driver, &guarded, "Apply and continue");
        guarded.drop_without_applying_deltas();
        assert!(driver.editor.properties_popup.is_none());
        assert!(driver.editor.pending_scope_change.is_none());
        let session = driver.editor.session.as_ref().expect("authoring session");
        assert_ne!(session.selection.as_ref(), Some(&target));
        let (menu_id, ring_id, cell_id) = cell_target_parts(&target);
        assert_eq!(
            authored_cell(&driver.editor, &menu_id, &ring_id, &cell_id).content,
            CellContent::Action { binding: pin },
            "Apply and continue persists the exact pinned binding before navigating"
        );
        assert!(continued.platform_output.accesskit_update.is_some());
        continued.drop_without_applying_deltas();
        pinned.drop_without_applying_deltas();
    }

    #[test]
    fn saving_query_after_selecting_result_clears_candidate_before_navigation_apply() {
        let (mut driver, target, initial) = retained_properties_popup();
        let stale_candidate = test_pinned_binding();
        let saved_query = "keep this query after choosing a result";
        seed_properties_search_result(&mut driver, &target, stale_candidate.clone(), saved_query);
        initial.drop_without_applying_deltas();
        let shown = driver.frame(Vec::new());
        let selected = click_retained_named_fragment(
            &mut driver,
            &shown,
            "Fixture action",
            egui::accesskit::Role::Button,
        );
        shown.drop_without_applying_deltas();
        assert!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .has_unassigned_text()
        );
        let saved = click_retained_button(&mut driver, &selected, "Save query");
        selected.drop_without_applying_deltas();
        let draft = driver.editor.properties_draft.as_ref().unwrap();
        assert_eq!(
            draft.action_binding,
            Some(ActionBinding::LauncherQuery {
                query: saved_query.into(),
                mode: crate::radial::model::QueryRunMode::OpenLauncher,
            })
        );
        assert_eq!(draft.action_editor.selected_binding(), None);
        assert!(
            !draft.action_editor.has_unassigned_text(),
            "explicit query assignment reconciles the highlighted result"
        );

        queue_navigation_to_screenshot_region(&mut driver);
        let guarded = driver.frame(Vec::new());
        let continued = click_retained_button(&mut driver, &guarded, "Apply and continue");
        guarded.drop_without_applying_deltas();
        assert!(driver.editor.properties_popup.is_none());
        assert!(driver.editor.pending_scope_change.is_none());
        let (menu_id, ring_id, cell_id) = cell_target_parts(&target);
        assert_eq!(
            authored_cell(&driver.editor, &menu_id, &ring_id, &cell_id).content,
            CellContent::Action {
                binding: ActionBinding::LauncherQuery {
                    query: saved_query.into(),
                    mode: crate::radial::model::QueryRunMode::OpenLauncher,
                },
            },
            "navigation Apply keeps the query selected by Save query, not the stale row"
        );
        assert!(continued.platform_output.accesskit_update.is_some());
        continued.drop_without_applying_deltas();
        saved.drop_without_applying_deltas();
        assert_ne!(
            authored_cell(&driver.editor, &menu_id, &ring_id, &cell_id).content,
            CellContent::Action {
                binding: stale_candidate,
            }
        );
    }

    #[test]
    fn skins_tab_preserves_inspector_text_and_skin_navigation_can_be_cancelled() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.tree_visible = true;
        editor.inspector_visible = true;
        editor.tree_width = 300.0;
        editor.inspector_width = 320.0;
        let (target, scope, assigned_binding) = {
            let session = editor.session.as_mut().expect("authoring session");
            let menu = session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == MenuId::new("starter-screen-tools"))
                .expect("screen tools menu");
            let ring = &menu.rings[0];
            let cell = ring
                .cells
                .iter()
                .find(|cell| cell.id == CellId::new("starter-screen-tools-draw"))
                .expect("screen draw action");
            let CellContent::Action { binding } = &cell.content else {
                panic!("screen draw cell is an action")
            };
            let assigned_binding = binding.clone();
            let target = StableSelection::Cell {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: cell.id.clone(),
            };
            session.select(Some(target.clone()));
            let scope = BindingEditorScope {
                surface: BindingEditorSurface::Inspector,
                editor_session: session.editor_session,
                draft_generation: session.generation,
                target: target.clone(),
                slot: BindingEditorSlot::CellPrimary,
            };
            (target, scope, assigned_binding)
        };
        editor.inspector_action_editor_scope = Some(scope.clone());
        editor
            .inspector_action_editor
            .seed_unassigned_query_for_test(
                scope.clone(),
                Some(&assigned_binding),
                "unsaved action text",
            );
        let mut driver = RetainedDesignerDriver::new(editor);
        driver
            .context
            .style_mut_of(driver.context.theme(), |style| style.animation_time = 0.0);
        let time = std::cell::Cell::new(0.0);
        let frame = |driver: &mut RetainedDesignerDriver, events| {
            let next_time = time.get() + 1.0 / 60.0;
            time.set(next_time);
            driver.frame_in_client(egui::vec2(1000.0, 1800.0), next_time, events)
        };

        let initial = frame(&mut driver, Vec::new());
        let skins = accesskit_named_bounds(&initial, "Skins", egui::accesskit::Role::Button);
        let point = skins.center();
        initial.drop_without_applying_deltas();
        frame(&mut driver, vec![egui::Event::PointerMoved(point)]).drop_without_applying_deltas();
        frame(&mut driver, vec![pointer_button_event(point, true)]).drop_without_applying_deltas();
        frame(&mut driver, vec![pointer_button_event(point, false)]).drop_without_applying_deltas();
        assert!(driver.editor.show_resources);
        assert!(driver.editor.inspector_action_editor.has_unassigned_text());
        assert_eq!(
            driver.editor.inspector_action_editor_scope,
            Some(scope.clone())
        );
        assert_eq!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&target)
        );

        // The gallery Grid learns its column widths during its first frame.
        // Capture the edit row from the next retained layout, then verify it
        // remains at the actual pointer target through press and release.
        let settled = frame(&mut driver, Vec::new());
        let skin =
            accesskit_named_bounds(&settled, "Edit Modern Clean", egui::accesskit::Role::Button);
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 1800.0))
                .contains_rect(skin),
            "Advanced edit owner bounds={skin:?}"
        );
        let point = skin.center();
        settled.drop_without_applying_deltas();
        let hovered = frame(&mut driver, vec![egui::Event::PointerMoved(point)]);
        assert_eq!(
            accesskit_named_bounds(&hovered, "Edit Modern Clean", egui::accesskit::Role::Button),
            skin,
            "retained edit row must stay under the pointer"
        );
        hovered.drop_without_applying_deltas();
        let pressed = frame(&mut driver, vec![pointer_button_event(point, true)]);
        assert_eq!(
            accesskit_named_bounds(&pressed, "Edit Modern Clean", egui::accesskit::Role::Button),
            skin,
            "retained edit row must stay under the press"
        );
        pressed.drop_without_applying_deltas();
        let release_time = time.get() + 1.0 / 60.0;
        time.set(release_time);
        let mut skin_clicked_in_child = false;
        let mut skin_release_used_child_viewport = false;
        let released = driver.frame_in_client_with_observer(
            egui::vec2(1000.0, 1800.0),
            release_time,
            vec![pointer_button_event(point, false)],
            |ctx, _| {
                skin_release_used_child_viewport =
                    ctx.viewport_id() == radial_designer_viewport_id();
                skin_clicked_in_child = ctx.input(|input| input.pointer.primary_clicked());
            },
        );
        released.drop_without_applying_deltas();
        assert!(skin_release_used_child_viewport);
        assert!(skin_clicked_in_child);
        assert!(
            driver.editor.pending_scope_change.is_some(),
            "Advanced row must queue its guarded scope change; bounds={skin:?}, selection={:?}, dirty={}",
            driver.editor.session.as_ref().unwrap().selection,
            driver.editor.inspector_action_editor.has_unassigned_text()
        );
        // The new guard Window uses its first frame to establish area size.
        // Read its actual controls after the next retained layout frame.
        let guarded = frame(&mut driver, Vec::new());
        let cancel =
            accesskit_named_bounds(&guarded, "Cancel change", egui::accesskit::Role::Button)
                .center();
        guarded.drop_without_applying_deltas();
        frame(&mut driver, vec![egui::Event::PointerMoved(cancel)]).drop_without_applying_deltas();
        frame(&mut driver, vec![pointer_button_event(cancel, true)]).drop_without_applying_deltas();
        let cancelled = frame(&mut driver, vec![pointer_button_event(cancel, false)]);
        assert!(driver.editor.pending_scope_change.is_none());
        assert!(driver.editor.show_resources);
        assert!(driver.editor.inspector_action_editor.has_unassigned_text());
        assert_eq!(
            driver.editor.inspector_action_editor_scope,
            Some(scope.clone())
        );
        assert_eq!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&target)
        );
        assert!(
            accesskit_named_bounds(&cancelled, "Menus", egui::accesskit::Role::Button)
                .is_positive()
        );

        cancelled.drop_without_applying_deltas();
        let menus_ready = frame(&mut driver, Vec::new());
        let menus_bounds =
            accesskit_named_bounds_if_present(&menus_ready, "Menus", egui::accesskit::Role::Button);
        menus_ready.drop_without_applying_deltas();
        let menus = menus_bounds
            .unwrap_or_else(|| panic!("current Menus button remains available after Cancel"));
        assert!(menus.is_positive());
        let menus = menus.center();

        // Observe the real parent owner and hit-tested layer on the same
        // retained child viewport before clicking the current native control.
        let next_time = time.get() + 1.0 / 60.0;
        time.set(next_time);
        let application_input_allowed = driver.frame.application_input_allowed;
        let has_pending_file_dialog = driver.frame.intent_bridge.has_pending_file_dialog();
        let mut menus_viewport_is_child = false;
        let mut menus_top_middle_layer = Some(egui::LayerId::background());
        let mut menus_hit_layer = None;
        let mut menus_native_owner_current = false;
        let hovered = driver.frame_in_client_with_observer(
            egui::vec2(1000.0, 1800.0),
            next_time,
            vec![egui::Event::PointerMoved(menus)],
            |ctx, _| {
                let parent_binding = NavigationViewportBinding {
                    viewport: radial_designer_viewport_id(),
                    owner: NavigationOwner::Dialog {
                        kind: NavigationDialogKind::RadialDesigner,
                        instance: 1,
                    },
                    base_layer: egui::LayerId::background(),
                };
                menus_viewport_is_child = ctx.viewport_id() == radial_designer_viewport_id();
                menus_top_middle_layer = visible_top_middle_layer(ctx);
                menus_hit_layer = ctx.layer_id_at(menus);
                menus_native_owner_current = application_input_allowed
                    && !has_pending_file_dialog
                    && parent_binding.base_is_current(ctx)
                    && !egui::Popup::is_any_open(ctx)
                    && parent_binding.focus_is_owned(ctx, None);
            },
        );
        let hovered_control =
            accesskit_named_control_if_present(&hovered, "Menus", egui::accesskit::Role::Button);
        hovered.drop_without_applying_deltas();
        assert!(menus_viewport_is_child);
        assert_eq!(menus_top_middle_layer, None);
        assert_eq!(menus_hit_layer, Some(egui::LayerId::background()));
        assert!(menus_native_owner_current);
        let (current_menus_bounds, menus_disabled) = hovered_control
            .unwrap_or_else(|| panic!("current Menus response is present after Cancel"));
        assert!(current_menus_bounds.is_positive());
        assert!(!menus_disabled, "the actual Menus response is enabled");
        assert_eq!(current_menus_bounds.center(), menus);
        let pressed = frame(&mut driver, vec![pointer_button_event(menus, true)]);
        let pressed_bounds =
            accesskit_named_bounds_if_present(&pressed, "Menus", egui::accesskit::Role::Button);
        pressed.drop_without_applying_deltas();
        assert_eq!(pressed_bounds, Some(current_menus_bounds));
        let back_in_menus = frame(&mut driver, vec![pointer_button_event(menus, false)]);
        let released_bounds = accesskit_named_bounds_if_present(
            &back_in_menus,
            "Menus",
            egui::accesskit::Role::Button,
        );
        back_in_menus.drop_without_applying_deltas();
        assert_eq!(released_bounds, Some(current_menus_bounds));
        assert!(!driver.editor.show_resources);
        assert!(driver.editor.inspector_action_editor.has_unassigned_text());
        assert_eq!(driver.editor.inspector_action_editor_scope, Some(scope));
        let in_menus = frame(&mut driver, Vec::new());
        let apply_bounds =
            accesskit_named_bounds_if_present(&in_menus, "Apply", egui::accesskit::Role::Button);
        in_menus.drop_without_applying_deltas();
        assert!(apply_bounds.is_some_and(|bounds| bounds.is_positive()));
    }

    #[test]
    fn inspector_dynamic_source_replacement_requires_explicit_action_decision() {
        let query = "keep this action before replacing its source";
        let (mut driver, target, original_binding, initial) = retained_dirty_inspector(query);
        let queued = click_retained_button(&mut driver, &initial, "Dynamic source");
        initial.drop_without_applying_deltas();
        assert!(matches!(
            driver
                .editor
                .pending_scope_change
                .as_ref()
                .map(|pending| &pending.intent),
            Some(ScopeChangeIntent::Operation(
                DeferredEditorOperation::SetCellContent {
                    content: CellContent::Dynamic {
                        source: DynamicSource::Favorites
                    },
                    ..
                }
            ))
        ));
        assert!(driver.editor.inspector_action_editor.has_unassigned_text());
        assert!(
            accesskit_named_bounds(&queued, "Apply action", egui::accesskit::Role::Button)
                .is_positive(),
            "a same-cell content replacement offers Apply action, not Apply and continue"
        );

        let applied = click_retained_button(&mut driver, &queued, "Apply action");
        queued.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        assert!(!driver.editor.inspector_action_editor.has_unassigned_text());
        let (menu_id, ring_id, cell_id) = cell_target_parts(&target);
        assert_eq!(
            authored_cell(&driver.editor, &menu_id, &ring_id, &cell_id).content,
            CellContent::Action {
                binding: ActionBinding::LauncherQuery {
                    query: query.into(),
                    mode: crate::radial::model::QueryRunMode::OpenLauncher,
                },
            },
            "Apply action saves the authored query but leaves the requested replacement unapplied"
        );
        assert_ne!(
            authored_cell(&driver.editor, &menu_id, &ring_id, &cell_id).content,
            CellContent::Action {
                binding: original_binding,
            }
        );

        let ready = driver.frame(Vec::new());
        let replaced = click_retained_button(&mut driver, &ready, "Dynamic source");
        ready.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        assert_eq!(
            authored_cell(&driver.editor, &menu_id, &ring_id, &cell_id).content,
            CellContent::Dynamic {
                source: DynamicSource::Favorites,
            },
            "repeating the source change after applying replaces the binding explicitly"
        );
        assert!(replaced.platform_output.accesskit_update.is_some());
        assert!(applied.platform_output.accesskit_update.is_some());
        replaced.drop_without_applying_deltas();
        applied.drop_without_applying_deltas();
    }

    #[test]
    fn retained_dirty_inspector_properties_handoff_requires_explicit_discard() {
        let (editor, target) = retained_inspector_editor();
        let mut driver = RetainedDesignerDriver::new(editor);
        let initial = driver.frame(Vec::new());
        let query = "Inspector browse query remains unassigned";
        replace_retained_text(&mut driver, &initial, "", query).drop_without_applying_deltas();
        initial.drop_without_applying_deltas();
        let before = driver.editor.acceptance_observation().unwrap();
        let draft = driver.editor.session.as_ref().unwrap().draft.clone();
        assert!(before.properties_dirty && !before.properties_popup_open);
        let request_properties = |driver: &mut RetainedDesignerDriver| {
            driver
                .editor
                .queue_navigation_intent(DesignerNavigationIntent::Select {
                    target: target.clone(),
                    control: false,
                    shift: false,
                    force_history: false,
                    open_properties: true,
                });
            driver.frame(Vec::new())
        };
        let guarded = request_properties(&mut driver);
        assert!(driver.editor.pending_scope_change.is_some());
        assert!(driver.editor.properties_popup.is_none());
        let cancelled = click_retained_button(&mut driver, &guarded, "Cancel change");
        guarded.drop_without_applying_deltas();
        cancelled.drop_without_applying_deltas();
        collect_retained_radial_parent_owner(&mut driver);
        assert!(driver.editor.pending_scope_change.is_none());
        assert_eq!(
            driver.editor.inspector_action_editor.query_for_test(),
            query
        );
        assert!(driver.editor.properties_popup_dirty());
        assert!(driver.editor.properties_popup.is_none());

        let guarded = request_properties(&mut driver);
        let discarded = click_retained_button(&mut driver, &guarded, "Discard and continue");
        guarded.drop_without_applying_deltas();
        discarded.drop_without_applying_deltas();
        collect_retained_radial_child_owner(&mut driver, RadialDesignerChildKind::CellProperties);
        let after = driver.editor.acceptance_observation().unwrap();
        assert!(driver.editor.pending_scope_change.is_none());
        assert_eq!(driver.editor.properties_popup, Some(target));
        assert!(after.properties_popup_open && after.properties_staged_digest.is_some());
        assert!(!after.properties_dirty);
        assert!(!driver.editor.inspector_action_editor.has_unassigned_text());
        let local_owner = after.action_editor.as_ref().unwrap();
        assert_eq!(local_owner.surface, "properties");
        assert_eq!(local_owner.editor_session_id, before.session_id);
        assert_eq!(local_owner.draft_generation, before.generation);
        assert_eq!(
            local_owner.assigned_binding_digest,
            before.assigned_binding_digest
        );
        assert!(!local_owner.search_pending && !local_owner.test_pending);
        assert_eq!(driver.editor.session.as_ref().unwrap().draft, draft);
        assert_eq!(
            (
                (
                    after.session_id,
                    after.generation,
                    after.document_digest,
                    after.assigned_binding_digest,
                    after.selected_target_digest,
                    after.selected_cell_digest,
                    after.selected_members_digest
                ),
                (
                    after.primary_target_digest,
                    after.range_anchor_target_digest,
                    after.navigation_path_digest,
                    after.pending_assets_digest,
                    after.undo_depth,
                    after.redo_depth,
                    after.draft_dirty
                )
            ),
            (
                (
                    before.session_id,
                    before.generation,
                    before.document_digest,
                    before.assigned_binding_digest,
                    before.selected_target_digest,
                    before.selected_cell_digest,
                    before.selected_members_digest
                ),
                (
                    before.primary_target_digest,
                    before.range_anchor_target_digest,
                    before.navigation_path_digest,
                    before.pending_assets_digest,
                    before.undo_depth,
                    before.redo_depth,
                    before.draft_dirty
                )
            ),
        );
        assert_eq!(
            after.selected_member_target_digests,
            before.selected_member_target_digests
        );
        assert_eq!(
            after.navigation_menu_id_digests,
            before.navigation_menu_id_digests
        );
        assert_eq!(
            after.navigation_edge_digests,
            before.navigation_edge_digests
        );
    }

    #[test]
    fn inspector_delete_guard_cancel_keeps_text_and_discard_continues_without_saving() {
        let query = "discard only after explicit choice";
        let (mut driver, target, original_binding, initial) = retained_dirty_inspector(query);
        let guarded = click_retained_button(&mut driver, &initial, "Delete cell");
        initial.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_some());
        assert!(driver.editor.inspector_action_editor.has_unassigned_text());
        let cancelled = click_retained_button(&mut driver, &guarded, "Cancel change");
        guarded.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        assert!(driver.editor.inspector_action_editor.has_unassigned_text());
        assert_eq!(
            driver.editor.inspector_action_editor.query_for_test(),
            query,
            "Cancel keeps the action input intact"
        );
        let (menu_id, ring_id, cell_id) = cell_target_parts(&target);
        assert_eq!(
            authored_cell(&driver.editor, &menu_id, &ring_id, &cell_id).content,
            CellContent::Action {
                binding: original_binding.clone(),
            }
        );
        assert!(cancelled.platform_output.accesskit_update.is_some());
        cancelled.drop_without_applying_deltas();

        let ready = driver.frame(Vec::new());
        let guarded = click_retained_button(&mut driver, &ready, "Delete cell");
        ready.drop_without_applying_deltas();
        let discarded = click_retained_button(&mut driver, &guarded, "Discard and continue");
        guarded.drop_without_applying_deltas();
        discarded.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        assert!(!driver.editor.inspector_action_editor.has_unassigned_text());
        let session = driver.editor.session.as_ref().expect("authoring session");
        let ring = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
            .expect("source ring remains authored");
        assert!(
            ring.cells.iter().all(|cell| cell.id != cell_id),
            "Discard and continue deletes the cell without assigning its unsaved query"
        );
    }

    #[test]
    fn inspector_cross_ring_swap_applies_query_to_the_moved_source_cell() {
        let query = "query follows the dragged cell";
        let (mut editor, target) = retained_inspector_editor();
        let (menu_id, source_ring_id, source_cell_id) = cell_target_parts(&target);
        let (destination_ring_id, destination_cell_id) = {
            let session = editor.session.as_mut().expect("authoring session");
            let destination_ring_id =
                menu::add_ring(session, &menu_id).expect("fixture can add an outer ring");
            let destination_cell_id = session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == menu_id)
                .and_then(|menu| {
                    menu.rings
                        .iter()
                        .find(|ring| ring.id == destination_ring_id)
                })
                .and_then(|ring| ring.cells.first())
                .expect("new ring has a destination slot")
                .id
                .clone();
            menu::set_cell_content(
                session,
                &menu_id,
                &destination_ring_id,
                &destination_cell_id,
                CellContent::Action {
                    binding: ActionBinding::ExactCommand {
                        command: "window:close".into(),
                        args: None,
                    },
                },
            )
            .expect("occupied slot fixture remains valid");
            session.select(Some(target.clone()));
            (destination_ring_id, destination_cell_id)
        };
        let original_binding = seed_inspector_query(&mut editor, &target, query);
        let generation = editor
            .session
            .as_ref()
            .expect("authoring session")
            .generation;
        editor.pending_drop = Some(PendingCellDrop {
            source_menu: menu_id.clone(),
            source_ring: source_ring_id.clone(),
            source_cell: source_cell_id.clone(),
            destination_menu: menu_id.clone(),
            destination_ring: destination_ring_id.clone(),
            destination_cell: destination_cell_id.clone(),
            destination_index: 0,
            generation,
        });
        let mut driver = RetainedDesignerDriver::new(editor);
        let initial = driver.frame(Vec::new());
        let swap_confirmation = click_retained_button(&mut driver, &initial, "Swap");
        initial.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_some());
        assert!(driver.editor.inspector_action_editor.has_unassigned_text());
        let continued =
            click_retained_button(&mut driver, &swap_confirmation, "Apply and continue");
        swap_confirmation.drop_without_applying_deltas();
        continued.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        assert!(!driver.editor.inspector_action_editor.has_unassigned_text());

        let moved = authored_cell(
            &driver.editor,
            &menu_id,
            &destination_ring_id,
            &source_cell_id,
        );
        assert_eq!(
            moved.content,
            CellContent::Action {
                binding: ActionBinding::LauncherQuery {
                    query: query.into(),
                    mode: crate::radial::model::QueryRunMode::OpenLauncher,
                },
            },
            "the applied action belongs to the source cell after the cross-ring swap"
        );
        assert_ne!(
            moved.content,
            CellContent::Action {
                binding: original_binding
            }
        );
        assert_eq!(
            authored_cell(
                &driver.editor,
                &menu_id,
                &source_ring_id,
                &destination_cell_id,
            )
            .content,
            CellContent::Action {
                binding: ActionBinding::ExactCommand {
                    command: "window:close".into(),
                    args: None,
                },
            },
            "the occupied destination action travels to the source slot"
        );
    }

    #[test]
    fn inspector_guard_defers_undo_and_redo_until_the_action_is_resolved() {
        let (mut editor, target) = retained_inspector_editor();
        let (menu_id, _, _) = cell_target_parts(&target);
        menu::rename_menu(
            editor.session.as_mut().expect("authoring session"),
            menu_id.clone(),
            "Undo must wait for the action decision".into(),
            EditPhase::Atomic,
        )
        .expect("fixture creates undo history");
        editor
            .session
            .as_mut()
            .expect("authoring session")
            .select(Some(target.clone()));
        let query = "preserve query while undo is pending";
        seed_inspector_query(&mut editor, &target, query);
        let mut driver = RetainedDesignerDriver::new(editor);
        driver.frame(Vec::new()).drop_without_applying_deltas();
        driver
            .editor
            .queue_editor_operation(DeferredEditorOperation::Undo);
        let guarded = driver.frame(Vec::new());
        assert!(driver.editor.pending_scope_change.is_some());
        let applied = click_retained_button(&mut driver, &guarded, "Apply action");
        guarded.drop_without_applying_deltas();
        applied.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        let session = driver.editor.session.as_ref().expect("authoring session");
        assert_eq!(
            session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == menu_id)
                .expect("menu remains")
                .name,
            "Undo must wait for the action decision",
            "the first click saves the action without running Undo"
        );
        let (menu_id, ring_id, cell_id) = cell_target_parts(&target);
        assert_eq!(
            authored_cell(&driver.editor, &menu_id, &ring_id, &cell_id).content,
            CellContent::Action {
                binding: ActionBinding::LauncherQuery {
                    query: query.into(),
                    mode: crate::radial::model::QueryRunMode::OpenLauncher,
                },
            }
        );

        let (mut editor, target) = retained_inspector_editor();
        let (menu_id, _, _) = cell_target_parts(&target);
        let original_name = editor
            .session
            .as_ref()
            .expect("authoring session")
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .expect("screen tools menu")
            .name
            .clone();
        menu::rename_menu(
            editor.session.as_mut().expect("authoring session"),
            menu_id.clone(),
            "Redo must wait for the action decision".into(),
            EditPhase::Atomic,
        )
        .expect("fixture creates redo history");
        let session = editor.session.as_mut().expect("authoring session");
        assert!(session.undo(), "fixture creates a redo entry");
        session.select(Some(target.clone()));
        let query = "preserve query while redo is pending";
        seed_inspector_query(&mut editor, &target, query);
        let mut driver = RetainedDesignerDriver::new(editor);
        driver.frame(Vec::new()).drop_without_applying_deltas();
        driver
            .editor
            .queue_editor_operation(DeferredEditorOperation::Redo);
        let guarded = driver.frame(Vec::new());
        assert!(driver.editor.pending_scope_change.is_some());
        let applied = click_retained_button(&mut driver, &guarded, "Apply action");
        guarded.drop_without_applying_deltas();
        applied.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        let session = driver.editor.session.as_ref().expect("authoring session");
        assert_eq!(
            session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == menu_id)
                .expect("menu remains")
                .name,
            original_name,
            "the first click saves the action without running Redo"
        );
        let (menu_id, ring_id, cell_id) = cell_target_parts(&target);
        assert_eq!(
            authored_cell(&driver.editor, &menu_id, &ring_id, &cell_id).content,
            CellContent::Action {
                binding: ActionBinding::LauncherQuery {
                    query: query.into(),
                    mode: crate::radial::model::QueryRunMode::OpenLauncher,
                },
            }
        );
    }

    #[test]
    fn scope_change_anchor_rejects_a_stale_session_generation_or_target() {
        let (mut editor, target) = retained_inspector_editor();
        let (menu_id, ring_id, cell_id) = cell_target_parts(&target);
        editor.queue_editor_operation(DeferredEditorOperation::CopyCell {
            menu_id: menu_id.clone(),
            ring_id,
            cell_id,
        });
        let pending = editor
            .pending_scope_change
            .as_ref()
            .expect("operation captures its anchor")
            .clone();
        assert!(editor.scope_change_is_current(&pending));

        let session = editor.session.as_mut().expect("authoring session");
        session.select(Some(StableSelection::Menu(menu_id)));
        assert!(
            !editor.scope_change_is_current(&pending),
            "target is part of the anchor"
        );

        let session = editor.session.as_mut().expect("authoring session");
        session.select(pending.anchor.selection.clone());
        session.generation =
            crate::radial::authoring::DraftGeneration(pending.anchor.generation.0.wrapping_add(1));
        assert!(
            !editor.scope_change_is_current(&pending),
            "draft generation is part of the anchor"
        );

        let session = editor.session.as_mut().expect("authoring session");
        session.generation = pending.anchor.generation;
        session.editor_session = crate::radial::authoring::AuthoringSessionId(
            pending.anchor.editor_session.0.wrapping_add(1),
        );
        assert!(
            !editor.scope_change_is_current(&pending),
            "editor session identity is part of the anchor"
        );
    }

    #[test]
    fn stale_inspector_scope_change_cannot_run_after_generation_changes() {
        let (mut driver, target, _, initial) =
            retained_dirty_inspector("stale edit must remain local");
        initial.drop_without_applying_deltas();
        let (menu_id, ring_id, cell_id) = cell_target_parts(&target);
        let before_count = authored_cell_count(&driver.editor, &menu_id, &ring_id);
        driver
            .editor
            .queue_editor_operation(DeferredEditorOperation::CopyCell {
                menu_id: menu_id.clone(),
                ring_id: ring_id.clone(),
                cell_id,
            });
        menu::rename_menu(
            driver.editor.session.as_mut().expect("authoring session"),
            menu_id.clone(),
            "Concurrent generation change".into(),
            EditPhase::Atomic,
        )
        .expect("fixture changes generation while the operation waits");
        let stale = driver.frame(Vec::new());
        assert!(driver.editor.pending_scope_change.is_some());
        assert!(driver.editor.inspector_action_editor.has_unassigned_text());
        assert!(
            accesskit_named_bounds(
                &stale,
                "Discard action and cancel stale change",
                egui::accesskit::Role::Button,
            )
            .is_positive()
        );
        let discarded = click_retained_button(
            &mut driver,
            &stale,
            "Discard action and cancel stale change",
        );
        stale.drop_without_applying_deltas();
        discarded.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        assert!(!driver.editor.inspector_action_editor.has_unassigned_text());
        assert_eq!(
            authored_cell_count(&driver.editor, &menu_id, &ring_id),
            before_count,
            "the stale Copy operation is discarded instead of targeting the newer draft"
        );
    }

    fn authored_cell_count(
        editor: &RadialEditorState,
        menu_id: &MenuId,
        ring_id: &RingId,
    ) -> usize {
        editor
            .session
            .as_ref()
            .expect("authoring session")
            .draft
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
            .expect("authored ring")
            .cells
            .len()
    }

    fn test_radial_add_trace_identity() -> RadialInsertionTraceIdentity {
        RadialInsertionTraceIdentity {
            request_id: 901,
            source_target_digest: 902,
            source_action_digest: 903,
            source_binding_digest: 904,
            source_query_digest: 905,
        }
    }

    fn test_radial_add_request() -> PendingRadialAdd {
        PendingRadialAdd {
            binding: crate::radial::model::ActionBinding::ExactCommand {
                command: "window:close".into(),
                args: Some("first argument".into()),
            },
            label: "Close window".into(),
            source_query: "close".into(),
            trace_identity: test_radial_add_trace_identity(),
        }
    }

    #[test]
    fn a_fresh_radial_add_cannot_replay_the_previous_request_reopen_receipt() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_ref().expect("test session");
        let menu = &session.draft.menus[0];
        let ring = &menu.rings[0];
        let cell = &ring.cells[0];
        let previous = RadialInsertionTraceContext {
            identity: test_radial_add_trace_identity(),
            menu_id: menu.id.clone(),
            ring_id: ring.id.clone(),
            cell_id: cell.id.clone(),
            prior_binding_digest: 906,
        };

        // A normal reopen after an Add arms the one legitimate Reopen receipt.
        editor.last_radial_add_trace = Some(previous.clone());
        editor.open = false;
        editor.open();
        assert!(editor.last_radial_add_reopen_pending);

        // Starting the next Add replaces that provenance before the new
        // insertion has been saved and reopened.
        let next = test_radial_add_request();
        editor.add_action_to_radial(
            next.binding,
            next.label,
            next.source_query,
            test_radial_add_trace_identity(),
        );
        assert!(editor.last_radial_add_trace.is_none());
        assert!(!editor.last_radial_add_reopen_pending);
        assert!(editor.radial_add_request.is_some());

        // The new request can still produce exactly one Reopen after its own
        // completed insertion is saved and the Designer is reopened.
        editor.last_radial_add_trace = Some(previous);
        editor.open = false;
        editor.open();
        assert!(editor.last_radial_add_reopen_pending);
    }

    #[test]
    fn inspector_text_edit_trace_comes_from_the_rendered_and_changed_text_edit() {
        acceptance_trace::reset_inspector_text_edit_test_state();
        let context = egui::Context::default();
        context.set_pixels_per_point(1.0);
        let menu_id = MenuId::new("trace-menu");
        let ring_id = RingId::new("trace-ring");
        let cell_id = CellId::new("trace-cell");
        let mut value = "Pinned label".to_owned();
        let response_bounds = std::cell::Cell::new(egui::Rect::NOTHING);
        let mut draw = |events: Vec<egui::Event>| {
            let client_size = egui::vec2(720.0, 480.0);
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, client_size)),
                events,
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .expect("root viewport is present in RawInput")
                .inner_rect = Some(egui::Rect::from_min_size(
                egui::pos2(241.0, -180.0),
                client_size,
            ));
            context.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = ui.text_edit_singleline(&mut value);
                    response_bounds.set(response.rect);
                    trace_inspector_cell_text_edit(
                        ui, &response, &menu_id, &ring_id, &cell_id, 73, 4, &value,
                    );
                });
            })
        };

        draw(Vec::new()).drop_without_applying_deltas();
        let point = response_bounds.get().center();
        draw(vec![egui::Event::PointerMoved(point)]).drop_without_applying_deltas();
        let pointer_button = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        draw(vec![pointer_button(true)]).drop_without_applying_deltas();
        draw(vec![pointer_button(false)]).drop_without_applying_deltas();
        draw(vec![egui::Event::Text(" edited".into())]).drop_without_applying_deltas();
        draw(Vec::new()).drop_without_applying_deltas();

        let expected_digest = acceptance_trace::private_trace_text_digest(&value);
        assert_eq!(value, "Pinned label edited");
        let events = acceptance_trace::take_inspector_text_edit_test_events();
        let edited = events.into_iter().rev().find_map(|event| match event {
            acceptance_trace::Event::DesignerInspectorCellTextEdit {
                target_digest,
                session_id,
                generation,
                value_digest,
                visible,
                fully_visible,
                focused,
                clicked,
                changed,
                ..
            } if changed => Some((
                target_digest,
                session_id,
                generation,
                value_digest,
                visible,
                fully_visible,
                focused,
                clicked,
            )),
            _ => None,
        });
        let (target, session, generation, digest, visible, fully_visible, focused, clicked) =
            edited.expect("actual TextEdit change emits a correlated owner observation");
        assert_eq!(
            target,
            acceptance_trace::private_trace_parts_digest(&[
                &menu_id.to_string(),
                &ring_id.to_string(),
                &cell_id.to_string(),
            ])
        );
        assert_eq!((session, generation, digest), (73, 4, expected_digest));
        assert!(visible && fully_visible && focused);
        assert!(!clicked);
    }

    #[test]
    fn authoring_control_clip_uses_the_actual_scroll_area_in_client_coordinates() {
        let client_size = egui::vec2(520.0, 380.0);
        let client = egui::Rect::from_min_size(egui::Pos2::ZERO, client_size);
        let mut snapshots = Vec::new();
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        // ScrollArea learns its content overflow on the first rendered frame.
        for (offset, observe) in [(0.0, false), (0.0, true), (120.0, true)] {
            let mut input = egui::RawInput {
                screen_rect: Some(client),
                ..Default::default()
            };
            let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
            viewport.native_pixels_per_point = Some(1.5);
            viewport.inner_rect = Some(egui::Rect::from_min_size(
                egui::pos2(241.0, -180.0),
                client_size,
            ));
            let output =
                ctx.run_ui(input, |ui| {
                    egui::Panel::top("pane-clip-toolbar")
                        .exact_size(64.0)
                        .show(ui, |ui| {
                            ui.label("Designer toolbar");
                        });
                    egui::Panel::right("pane-clip-inspector")
                        .exact_size(200.0)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                        .max_height(150.0)
                        .auto_shrink([false, false])
                        .vertical_scroll_offset(offset)
                        .show(ui, |ui| {
                            ui.add_space(160.0);
                            let mut label = String::from("Bulk label");
                            let response =
                                ui.add(egui::TextEdit::singleline(&mut label).desired_width(150.0));
                            let bounds = designer_authoring_clip_bounds(ui, client_size).unwrap();
                            let clip = ui.clip_rect().intersect(client);
                            let scale =
                                |value: f32| (value * ui.ctx().pixels_per_point()).round() as i32;
                            assert_eq!(
                                bounds,
                                [
                                    scale(clip.left()),
                                    scale(clip.top()),
                                    scale(clip.right()),
                                    scale(clip.bottom())
                                ]
                            );
                            assert!(
                                client.contains_rect(response.rect),
                                "offset={offset} response={:?} clip={clip:?} client={client:?}",
                                response.rect,
                            );
                            if observe {
                                assert!(clip.top() > 0.0 && clip.bottom() < client.bottom());
                                assert_eq!(clip.contains_rect(response.rect), offset > 0.0);
                                snapshots.push((response.rect, bounds));
                            }
                            ui.add_space(400.0);
                        });
                        });
                });
            output.drop_without_applying_deltas();
        }
        assert_eq!(
            snapshots[0].1, snapshots[1].1,
            "scrolling retains the measured pane clip"
        );
        assert!(
            snapshots[1].0.top() < snapshots[0].0.top(),
            "the real control moves into its clip"
        );
    }

    #[test]
    fn retained_resources_scroll_geometry_preserves_authoring_state() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_mut().unwrap();
        let menu_id = session.draft.default_menu_id.clone();
        session.select(Some(StableSelection::Menu(menu_id)));
        let before = (
            session.draft.clone(),
            session.generation,
            session.selection.clone(),
            session.acceptance_history_depths(),
            session.pending_assets.clone(),
        );
        let path = editor.visited_path.current().cloned();
        let context = egui::Context::default();
        context.enable_accesskit();
        context.style_mut_of(context.theme(), |style| style.animation_time = 0.0);
        let client_size = egui::vec2(900.0, 650.0);
        let mut frame = |offset| {
            let mut clip = None;
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, client_size)),
                    ..Default::default()
                },
                |ctx| {
                    egui::Panel::top("retained-resources-toolbar")
                        .exact_size(107.0)
                        .show(ctx, |ui| {
                            ui.label("Designer toolbar");
                        });
                    egui::CentralPanel::default().show(ctx, |ui| {
                        // Mount the actual Resources UI under its production
                        // vertical scroll owner; no substitute controls.
                        egui::ScrollArea::vertical()
                            .id_salt("radial-designer-resources")
                            .max_height(400.0)
                            .vertical_scroll_offset(offset)
                            .show(ui, |ui| {
                                clip = designer_authoring_clip_bounds(ui, client_size);
                                editor.resources_ui(
                                    ui,
                                    ViewportClass::Deferred,
                                    &mut RadialUiPass::default(),
                                );
                            });
                    });
                },
            );
            let below = accesskit_named_bounds(&output, "−", egui::accesskit::Role::Button);
            output.drop_without_applying_deltas();
            (below, clip.unwrap())
        };
        // The real gallery Grid and ScrollArea retain their measured widths
        // and overflow after the first frame.
        let _ = frame(0.0);
        let (below, clip) = frame(0.0);
        assert!(below.bottom() > clip[3] as f32, "{below:?}, clip={clip:?}");
        let visible_offset = below.top() - clip[1] as f32 - 12.0;
        assert!(visible_offset > 0.0);
        let (visible, visible_clip) = frame(visible_offset);
        assert_eq!(visible_clip, clip);
        assert!(visible.top() >= clip[1] as f32 && visible.bottom() <= clip[3] as f32);
        assert!(visible.top() < below.top());
        let (above, above_clip) = frame(visible_offset + 18.0);
        assert_eq!(above_clip, clip);
        assert!(above.top() < clip[1] as f32 && above.bottom() > clip[1] as f32);
        let (restored, restored_clip) = frame(visible_offset);
        assert_eq!(restored_clip, clip);
        assert_eq!(restored, visible);
        let session = editor.session.as_ref().unwrap();
        assert_eq!(
            before,
            (
                session.draft.clone(),
                session.generation,
                session.selection.clone(),
                session.acceptance_history_depths(),
                session.pending_assets.clone(),
            )
        );
        assert_eq!(editor.visited_path.current().cloned(), path);
        assert!(editor.pending_scope_change.is_none());
        assert!(editor.post_render.is_empty());
    }

    #[test]
    fn retained_menu_tree_scroll_geometry_preserves_authoring_state() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_mut().unwrap();
        let original_selection = session.selection.clone();
        for index in 0..40 {
            let id = format!("scroll-owner-menu-{index:02}");
            menu::create_menu(session, &id, &id).unwrap();
        }
        session.select(original_selection);
        let before = (
            session.draft.clone(),
            session.generation,
            session.selection.clone(),
            session.acceptance_history_depths(),
            session.pending_assets.clone(),
        );
        let path = editor.visited_path.current().cloned();
        let first_name = session.draft.menus[0].name.clone();
        let context = egui::Context::default();
        context.enable_accesskit();
        context.style_mut_of(context.theme(), |style| style.animation_time = 0.0);
        let mut scroll_id = None;
        let mut frame = |offset: Option<f32>| {
            if let Some(offset) = offset {
                let id = scroll_id.expect("actual tree ScrollArea retained its state");
                let mut state = egui::scroll_area::State::load(&context, id).unwrap();
                state.offset.y = offset;
                state.store(&context, id);
            }
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        scroll_id = Some(ui.make_persistent_id(egui::IdSalt::new("scroll_area")));
                        editor.tree(
                            ui,
                            &crate::radial::model::RadialFeatureSettings::default(),
                            ViewportClass::Deferred,
                            &RadialUiPass::default(),
                        );
                    });
                },
            );
            let clip = output
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::epaint::Shape::Text(text) = &shape.shape {
                        let name = text.galley.text();
                        if name == first_name || name.starts_with("scroll-owner-menu-") {
                            return Some(shape.clip_rect);
                        }
                    }
                    None
                })
                .expect("actual painted menu row supplies its ScrollArea clip");
            let bounds =
                |name: &str| accesskit_named_bounds(&output, name, egui::accesskit::Role::Button);
            let below = bounds("scroll-owner-menu-39");
            let first = bounds(&first_name);
            output.drop_without_applying_deltas();
            (below, first, clip)
        };
        let _ = frame(None);
        let (below, first, clip) = frame(None);
        assert!(clip.contains_rect(first));
        assert!(below.top() > clip.bottom());
        let visible_offset = below.bottom() - clip.bottom() + 8.0;
        let (visible, above, visible_clip) = frame(Some(visible_offset));
        assert_eq!(visible_clip, clip);
        assert!(clip.contains_rect(visible));
        assert!(visible.top() < below.top());
        assert!(above.bottom() < clip.top());
        let (below_again, first_again, restored_clip) = frame(Some(0.0));
        assert_eq!(restored_clip, clip);
        assert_eq!((below_again, first_again), (below, first));
        let session = editor.session.as_ref().unwrap();
        assert_eq!(
            before,
            (
                session.draft.clone(),
                session.generation,
                session.selection.clone(),
                session.acceptance_history_depths(),
                session.pending_assets.clone(),
            )
        );
        assert_eq!(editor.visited_path.current().cloned(), path);
        assert!(editor.tree_filter.is_empty());
        assert!(editor.pending_scope_change.is_none());
        assert!(editor.post_render.is_empty());
    }

    fn assert_retained_scroll_input_admission(
        mut driver: RetainedDesignerDriver,
        owner: DesignerAuthoringScrollOwner,
        client_size: egui::Vec2,
        control_bounds: impl Fn(&egui::FullOutput) -> egui::Rect,
    ) {
        driver
            .context
            .style_mut_of(driver.context.theme(), |style| style.animation_time = 0.0);
        let session = driver.editor.session.as_ref().unwrap();
        let before = (
            session.draft.clone(),
            session.generation,
            session.selection.clone(),
            session.acceptance_history_depths(),
            session.pending_assets.clone(),
        );
        let expected_menu = selected_menu_id(session).expect("fixture selects an authored menu");
        let mut time = 0.0;
        let mut frame = |driver: &mut RetainedDesignerDriver, events| {
            let _ = acceptance_trace::take_designer_scroll_viewport_test_events();
            // Real egui wheel smoothing completes within four retained frames
            // at this deterministic frame interval; no offset is set by the test.
            time += 0.1;
            let output = driver.frame_in_client(client_size, time, events);
            let measured = acceptance_trace::take_designer_scroll_viewport_test_events()
                .into_iter()
                .find(|event| event.owner == owner)
                .expect("actual ScrollArea published its measured input viewport");
            assert!(measured.is_valid());
            assert_eq!(
                measured.session_id,
                driver.editor.session.as_ref().unwrap().editor_session.0
            );
            assert_eq!(
                measured.generation,
                driver.editor.session.as_ref().unwrap().generation.0
            );
            (output, measured)
        };
        let (first_output, _) = frame(&mut driver, Vec::new());
        first_output.drop_without_applying_deltas();
        let (initial, measured) = frame(&mut driver, Vec::new());
        // viewport_ui establishes the selected menu's navigation before input.
        assert_eq!(driver.editor.visited_path.current(), Some(&expected_menu));
        let path = driver.editor.visited_path.clone();
        let initial_bounds = control_bounds(&initial);
        initial.drop_without_applying_deltas();
        let paint_left = measured.paint_clip_bounds[0] as f32;
        let input_left = measured.input_bounds[0] as f32;
        assert!(paint_left.is_finite() && input_left.is_finite());
        assert!(
            paint_left < input_left,
            "actual pane has a paint-only gap: {measured:?}"
        );
        let interior_top = (measured.paint_clip_bounds[1].max(measured.input_bounds[1])) as f32;
        let interior_bottom = (measured.paint_clip_bounds[3].min(measured.input_bounds[3])) as f32;
        assert!(interior_bottom > interior_top);
        let margin = egui::pos2(
            (paint_left + input_left) / 2.0,
            (interior_top + interior_bottom) / 2.0,
        );
        assert!(
            margin.x > paint_left && margin.x < input_left,
            "the point lies in the measured left paint-only gap: {measured:?}"
        );
        assert!(
            margin.y > measured.paint_clip_bounds[1] as f32
                && margin.y < measured.paint_clip_bounds[3] as f32
                && margin.y > measured.input_bounds[1] as f32
                && margin.y < measured.input_bounds[3] as f32,
            "the point stays within the measured paint and client-height bounds: {measured:?}"
        );
        let mut last_frame = measured.frame_nr;
        for index in 0..4 {
            let events = if index == 0 {
                vec![
                    egui::Event::PointerMoved(margin),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -50.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else {
                Vec::new()
            };
            let (output, current) = frame(&mut driver, events);
            assert!(current.frame_nr > last_frame);
            assert_eq!(current.scroll_id, measured.scroll_id);
            assert_eq!(current.input_bounds, measured.input_bounds);
            let current_bounds = control_bounds(&output);
            assert_eq!(
                current_bounds, initial_bounds,
                "wheel input in the actual left paint-only gap cannot scroll {owner:?}"
            );
            output.drop_without_applying_deltas();
            last_frame = current.frame_nr;
        }
        let interior = egui::pos2(
            (measured.input_bounds[0] + measured.input_bounds[2]) as f32 / 2.0,
            (measured.input_bounds[1] + measured.input_bounds[3]) as f32 / 2.0,
        );
        assert!(interior.x > measured.input_bounds[0] as f32);
        assert!(interior.x < measured.input_bounds[2] as f32);
        assert!(interior.y > measured.input_bounds[1] as f32);
        assert!(interior.y < measured.input_bounds[3] as f32);
        let mut final_bounds = initial_bounds;
        for index in 0..4 {
            let events = if index == 0 {
                vec![
                    egui::Event::PointerMoved(interior),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -50.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else {
                Vec::new()
            };
            let (output, current) = frame(&mut driver, events);
            assert!(current.frame_nr > last_frame);
            assert_eq!(current.scroll_id, measured.scroll_id);
            assert_eq!(current.paint_clip_bounds, measured.paint_clip_bounds);
            final_bounds = control_bounds(&output);
            output.drop_without_applying_deltas();
            last_frame = current.frame_nr;
        }
        assert!(
            (initial_bounds.top() - final_bounds.top() - 50.0).abs() < 0.5,
            "real wheel input must move the control, owner={owner:?} before={initial_bounds:?} after={final_bounds:?}"
        );
        let session = driver.editor.session.as_ref().unwrap();
        assert_eq!(
            before,
            (
                session.draft.clone(),
                session.generation,
                session.selection.clone(),
                session.acceptance_history_depths(),
                session.pending_assets.clone(),
            )
        );
        assert_eq!(driver.editor.visited_path, path);
        assert!(driver.editor.pending_scope_change.is_none());
        assert!(driver.editor.post_render.is_empty());
    }

    #[test]
    fn retained_resources_scroll_input_rejects_paint_margin_and_accepts_measured_interior() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_mut().unwrap();
        let menu_id = session.draft.default_menu_id.clone();
        menu::rename_menu(
            session,
            menu_id.clone(),
            "Unrelated dirty menu".into(),
            EditPhase::Atomic,
        )
        .unwrap();
        session.select(Some(StableSelection::Menu(menu_id)));
        editor.show_resources = true;
        editor.tree_visible = false;
        editor.inspector_visible = false;
        assert_retained_scroll_input_admission(
            RetainedDesignerDriver::new(editor),
            DesignerAuthoringScrollOwner::Resources,
            egui::vec2(900.0, 650.0),
            |output| {
                let update = output.platform_output.accesskit_update.as_ref().unwrap();
                let bounds = update
                    .nodes
                    .iter()
                    .filter(|(_, node)| {
                        node.role() == egui::accesskit::Role::Button && node.label() == Some("+")
                    })
                    .nth(1)
                    .expect("actual SimpleScale plus button follows the Opacity step buttons")
                    .1
                    .bounds()
                    .unwrap();
                egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                )
            },
        );
    }

    #[test]
    fn retained_tree_and_inspector_scroll_input_reject_paint_margin_and_preserve_state() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_mut().unwrap();
        let selection = Some(StableSelection::Menu(session.draft.default_menu_id.clone()));
        for index in 0..40 {
            let id = format!("scroll-input-menu-{index:02}");
            menu::create_menu(session, &id, &id).unwrap();
        }
        session.select(selection);
        editor.tree_visible = true;
        editor.inspector_visible = false;
        assert_retained_scroll_input_admission(
            RetainedDesignerDriver::new(editor),
            DesignerAuthoringScrollOwner::MenuTree,
            egui::vec2(900.0, 380.0),
            |output| {
                accesskit_named_bounds(
                    output,
                    "scroll-input-menu-39",
                    egui::accesskit::Role::Button,
                )
            },
        );
        let inspector = retained_bulk_style_driver("text.bold");
        assert_eq!(
            inspector
                .editor
                .session
                .as_ref()
                .unwrap()
                .selection
                .as_ref()
                .expect("Inspector fixture selects the two authored cells")
                .selected_cells()
                .len(),
            2
        );
        assert_retained_scroll_input_admission(
            inspector,
            DesignerAuthoringScrollOwner::Inspector,
            egui::vec2(900.0, 380.0),
            |output| {
                accesskit_named_bounds(
                    output,
                    "Set label on 2 cells",
                    egui::accesskit::Role::Button,
                )
            },
        );
    }

    #[test]
    fn measured_scroll_input_viewport_respects_auto_shrink_and_fractional_client_pixels() {
        for (content_size, fractional) in [
            (egui::vec2(90.0, 40.0), false),
            (egui::vec2(700.0, 900.0), true),
        ] {
            let context = egui::Context::default();
            context.set_pixels_per_point(if fractional { 1.5 } else { 1.0 });
            let client_size = egui::vec2(520.0, 380.0);
            let mut measured = None;
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, client_size)),
                        viewports: [(
                            egui::ViewportId::ROOT,
                            egui::ViewportInfo {
                                inner_rect: Some(egui::Rect::from_min_size(
                                    egui::pos2(-640.0, 120.0),
                                    client_size,
                                )),
                                native_pixels_per_point: Some(if fractional { 1.5 } else { 1.0 }),
                                ..Default::default()
                            },
                        )]
                        .into_iter()
                        .collect(),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let mut paint_clip = egui::Rect::NOTHING;
                            let output = egui::ScrollArea::vertical().show(ui, |ui| {
                                paint_clip = ui.clip_rect();
                                ui.allocate_exact_size(content_size, egui::Sense::hover());
                            });
                            let receipt = designer_scroll_viewport_measurement(
                                ui,
                                &output,
                                paint_clip,
                                DesignerAuthoringScrollOwner::Resources,
                                Correlation {
                                    session_id: 7,
                                    generation: 9,
                                    ..Default::default()
                                },
                            )
                            .unwrap();
                            let scale = ui.ctx().pixels_per_point();
                            assert!(
                                receipt.input_bounds[0] as f32 >= output.inner_rect.left() * scale
                            );
                            assert!(
                                receipt.input_bounds[1] as f32 >= output.inner_rect.top() * scale
                            );
                            assert!(
                                receipt.input_bounds[2] as f32
                                    <= (output.inner_rect.left() + output.content_size.x)
                                        .min(output.inner_rect.right())
                                        * scale
                            );
                            assert!(
                                receipt.input_bounds[3] as f32
                                    <= (output.inner_rect.top() + output.content_size.y)
                                        .min(output.inner_rect.bottom())
                                        * scale
                            );
                            measured = Some(receipt);
                        });
                    },
                )
                .drop_without_applying_deltas();
            assert!(measured.unwrap().is_valid());
        }
    }

    #[test]
    fn rendered_search_text_observation_uses_the_painted_galley_and_rejects_clipping() {
        let text = "A distinguishable long authored label  ·  Child · Ring ring-id · Cell cell-id";
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(520.0, 380.0));
        for (clipped, elided) in [(false, false), (true, false), (false, true)] {
            let ctx = egui::Context::default();
            let mut input = egui::RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .inner_rect = Some(viewport);
            let mut observed = None;
            let output = ctx.run_ui(input, |ui| {
                egui::Panel::left("search-results-test")
                    .exact_size(180.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("radial-designer-search-results")
                            .show(ui, |ui| {
                                if clipped {
                                    ui.set_clip_rect(egui::Rect::from_min_size(
                                        ui.min_rect().min,
                                        egui::vec2(ui.available_width(), 16.0),
                                    ));
                                }
                                let galley = egui::WidgetText::from(text).into_galley(
                                    ui,
                                    None,
                                    ui.available_width() - 2.0 * ui.spacing().button_padding.x,
                                    egui::TextStyle::Button,
                                );
                                assert!(
                                    galley.rows.len() > 1,
                                    "the actual search layout wraps the long result"
                                );
                                let galley = if elided {
                                    let mut job = galley.job.as_ref().clone();
                                    job.wrap.max_rows = 1;
                                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                                } else {
                                    galley
                                };
                                let response =
                                    ui.selectable_label(false, std::sync::Arc::clone(&galley));
                                observed = designer_rendered_text(ui, &response, &galley);
                            });
                    });
            });
            output.drop_without_applying_deltas();
            let observation = observed.expect("painted text owner observation");
            assert_eq!(
                observation.digest,
                acceptance_observation_digest(text.as_bytes())
            );
            assert!(observation.bounds[2] > observation.bounds[0]);
            assert!(observation.bounds[3] > observation.bounds[1]);
            assert_eq!(observation.fully_visible, !clipped);
            assert_eq!(observation.elided, elided);
        }
    }

    #[test]
    fn acceptance_observation_distinguishes_menu_selection_from_no_selection() {
        use crate::gui::query_observation::AuthoringSelectionKind;
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_mut().expect("authoring owner");
        let menu_id = session.draft.menus[0].id.clone();
        session.select(Some(StableSelection::Menu(menu_id)));
        let menu = editor.acceptance_observation().unwrap();
        assert_eq!(menu.selection_kind, AuthoringSelectionKind::Menu);
        assert_ne!(menu.selected_target_digest, 0);
        assert_eq!(menu.selected_member_count, 0);
        assert!(menu.selected_member_target_digests.is_empty());
        assert_eq!(
            (
                menu.selected_members_digest,
                menu.primary_cell_digest,
                menu.range_anchor_digest
            ),
            (0, 0, 0)
        );
        assert_eq!(
            (
                menu.primary_target_digest,
                menu.range_anchor_target_digest,
                menu.selected_cell_digest
            ),
            (0, 0, 0)
        );

        editor.session.as_mut().unwrap().select(None);
        let none = editor.acceptance_observation().unwrap();
        assert_eq!(none.selection_kind, AuthoringSelectionKind::None);
        assert_eq!(none.selected_target_digest, 0);
        assert_eq!(none.selected_member_count, 0);
        assert!(none.selected_member_target_digests.is_empty());
    }

    #[test]
    fn acceptance_observation_hashes_real_staged_properties_without_exposing_text() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_ref().expect("test authoring session");
        let menu = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let cell = session.draft.menus[0].rings[0].cells[0].clone();
        let target = StableSelection::Cell {
            menu_id: menu,
            ring_id: ring,
            cell_id: cell.id.clone(),
        };
        let mut properties = PropertiesDraft::from_cell(target.clone(), session.generation, &cell);
        properties.action_binding = Some(crate::radial::model::ActionBinding::LauncherQuery {
            query: "private authoring query".into(),
            mode: crate::radial::model::QueryRunMode::OpenLauncher,
        });
        editor.properties_popup = Some(target);
        editor.properties_draft = Some(properties);

        let observation = editor.acceptance_observation().expect("owner snapshot");
        assert!(observation.open);
        assert_ne!(observation.session_id, 0);
        assert_ne!(observation.generation, 0);
        assert_ne!(observation.selected_target_digest, 0);
        assert_ne!(observation.selected_cell_digest, 0);
        assert_ne!(observation.document_digest, 0);
        assert_ne!(observation.assigned_binding_digest, 0);
        assert!(!observation.draft_dirty);
        assert!(observation.properties_dirty);
        assert!(!observation.initial_snapshot_pending);
        let serialized = serde_json::to_string(&observation).unwrap();
        assert!(!serialized.contains("private authoring query"));
        let baseline_staged_digest = observation
            .properties_staged_digest
            .expect("active staged Properties fields are represented");
        let baseline_document = observation.document_digest;
        let baseline_depths = (observation.undo_depth, observation.redo_depth);
        let draft_snapshot =
            std::sync::Arc::clone(&editor.session.as_ref().expect("session").draft);

        let original_label = editor
            .properties_draft
            .as_ref()
            .expect("Properties draft")
            .label
            .clone();
        editor
            .properties_draft
            .as_mut()
            .expect("Properties draft")
            .label
            .push_str(" staged label");
        let label_digest = editor
            .acceptance_observation()
            .expect("label snapshot")
            .properties_staged_digest
            .unwrap();
        assert_ne!(label_digest, baseline_staged_digest);
        let properties = editor.properties_draft.as_mut().unwrap();
        properties.label = original_label;
        properties.dynamic_source = properties.dynamic_source.wrapping_add(1);
        let dynamic_digest = editor
            .acceptance_observation()
            .expect("dynamic-source snapshot")
            .properties_staged_digest
            .unwrap();
        assert_ne!(dynamic_digest, baseline_staged_digest);
        let properties = editor.properties_draft.as_mut().unwrap();
        properties.dynamic_source = 0;
        properties.icon_kind = 1;
        let icon_digest = editor
            .acceptance_observation()
            .expect("icon snapshot")
            .properties_staged_digest
            .unwrap();
        assert_ne!(icon_digest, baseline_staged_digest);
        let after = editor.acceptance_observation().expect("final snapshot");
        assert_eq!(after.document_digest, baseline_document);
        assert_eq!((after.undo_depth, after.redo_depth), baseline_depths);
        assert!(std::sync::Arc::ptr_eq(
            &editor.session.as_ref().expect("session").draft,
            &draft_snapshot
        ));
    }

    #[test]
    fn acceptance_observation_omits_cached_editor_after_selection_close_and_reopen() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.inspector_visible = true;
        let (scope, alternate_target) = {
            let session = editor.session.as_mut().expect("session");
            let menu = &session.draft.menus[0];
            let ring = &menu.rings[0];
            let target = StableSelection::Cell {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: ring.cells[0].id.clone(),
            };
            let alternate_target = StableSelection::Cell {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: ring.cells[1].id.clone(),
            };
            session.selection = Some(target.clone());
            (
                BindingEditorScope {
                    surface: BindingEditorSurface::Inspector,
                    editor_session: session.editor_session,
                    draft_generation: session.generation,
                    target,
                    slot: BindingEditorSlot::CellPrimary,
                },
                alternate_target,
            )
        };
        editor.inspector_action_editor_scope = Some(scope.clone());
        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 520.0),
            )),
            ..Default::default()
        };
        context
            .run_ui(input, |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let binding = crate::radial::model::ActionBinding::LauncherQuery {
                        query: "fixture query".into(),
                        mode: crate::radial::model::QueryRunMode::OpenLauncher,
                    };
                    let _ = editor.inspector_action_editor.show(
                        ui,
                        scope.clone(),
                        Some(&binding),
                        &InvocationContext::empty(0),
                    );
                });
            })
            .drop_without_applying_deltas();
        assert!(
            editor
                .acceptance_observation()
                .expect("live observer snapshot")
                .action_editor
                .is_some()
        );

        editor.session.as_mut().expect("session").selection = Some(alternate_target);
        assert!(
            editor
                .acceptance_observation()
                .expect("changed-selection snapshot")
                .action_editor
                .is_none()
        );

        editor.request_close();
        assert!(!editor.open);
        assert!(
            editor
                .acceptance_observation()
                .expect("closed snapshot")
                .action_editor
                .is_none()
        );
        editor.open_test_snapshot();
        assert!(
            editor
                .acceptance_observation()
                .expect("reopened session snapshot")
                .action_editor
                .is_none()
        );
    }

    fn test_action_editor_identity(epoch: u64) -> AuthoringBindingEditorIdentity {
        AuthoringBindingEditorIdentity {
            scope: BindingEditorScope {
                surface: BindingEditorSurface::Inspector,
                editor_session: crate::radial::authoring::AuthoringSessionId(3),
                draft_generation: crate::radial::authoring::DraftGeneration(5),
                target: StableSelection::Cell {
                    menu_id: MenuId::new("test-menu"),
                    ring_id: RingId::new("test-ring"),
                    cell_id: CellId::new("test-cell"),
                },
                slot: BindingEditorSlot::CellPrimary,
            },
            assigned_binding_digest: 41,
            editor_epoch: epoch,
            edit_generation: 7,
            query_generation: 11,
            query_request_generation: 13,
            search_request_generation: 17,
            test_request_generation: 19,
        }
    }

    fn test_provider_search_request(
        identity: AuthoringBindingEditorIdentity,
        query: &str,
        cancellation: Arc<std::sync::atomic::AtomicBool>,
    ) -> crate::gui::AuthoringProviderSearchRequest {
        crate::gui::AuthoringProviderSearchRequest {
            identity,
            query: query.into(),
            purpose: crate::gui::AuthoringProviderSearchPurpose::Preview,
            retry_attempt: 0,
            cancellation,
        }
    }

    #[test]
    fn replaced_queued_authoring_request_is_cancelled_and_new_identity_survives() {
        let _ = acceptance_trace::take_authoring_provider_trace_test_events();
        let bridge = DesignerIntentBridge::default();
        let first_identity = test_action_editor_identity(23);
        let first_cancellation = bridge.begin_action_search(first_identity.clone());
        let first = test_provider_search_request(
            first_identity.clone(),
            "first query",
            Arc::clone(&first_cancellation),
        );
        bridge
            .queue_authoring_search(first, Duration::from_secs(1))
            .expect("first bounded request queued");

        let replacement_identity = test_action_editor_identity(29);
        let replacement_cancellation = bridge.begin_action_search(replacement_identity.clone());
        let replacement = test_provider_search_request(
            replacement_identity.clone(),
            "replacement query",
            Arc::clone(&replacement_cancellation),
        );
        bridge
            .queue_authoring_search(replacement, Duration::ZERO)
            .expect("replacement supersedes matching queued request");
        assert!(first_cancellation.load(Ordering::Acquire));
        let cancellation_events = acceptance_trace::take_authoring_provider_trace_test_events();
        assert_eq!(cancellation_events.len(), 1);
        assert_eq!(cancellation_events[0].edge, "cancelled");
        assert_eq!(cancellation_events[0].kind, "search");
        assert_eq!(cancellation_events[0].identity, first_identity);
        assert_eq!(
            cancellation_events[0].query_digest,
            acceptance_trace::private_trace_text_digest("first query")
        );

        let (ready, next_due) = bridge.take_ready_authoring_search(Instant::now());
        let ready = ready.expect("replacement is ready immediately");
        assert_eq!(ready.identity, replacement_identity);
        assert_eq!(ready.query, "replacement query");
        assert!(next_due.is_none());
    }

    #[test]
    fn full_authoring_search_queue_returns_rejected_request_without_mutating_pending_entries() {
        let bridge = DesignerIntentBridge::default();
        let mut pending_identities = Vec::new();
        for index in 0..8 {
            let mut identity = test_action_editor_identity(40 + index);
            identity.scope.editor_session = crate::radial::authoring::AuthoringSessionId(index + 1);
            let cancellation = bridge.begin_action_search(identity.clone());
            let request = test_provider_search_request(
                identity.clone(),
                &format!("pending {index}"),
                Arc::clone(&cancellation),
            );
            bridge
                .queue_authoring_search(request, Duration::from_secs(60))
                .expect("bounded queue accepts its first eight requests");
            pending_identities.push(identity);
        }

        let mut rejected_identity = test_action_editor_identity(99);
        rejected_identity.scope.editor_session = crate::radial::authoring::AuthoringSessionId(99);
        let rejected_cancellation = bridge.begin_action_search(rejected_identity.clone());
        let rejected = test_provider_search_request(
            rejected_identity.clone(),
            "rejected ninth request",
            Arc::clone(&rejected_cancellation),
        );
        let rejected = bridge
            .queue_authoring_search(rejected, Duration::ZERO)
            .expect_err("a ninth distinct request is rejected");

        assert_eq!(rejected.identity, rejected_identity);
        assert_eq!(rejected.query, "rejected ninth request");
        assert!(Arc::ptr_eq(&rejected.cancellation, &rejected_cancellation));
        assert!(!rejected.cancellation.load(Ordering::Acquire));

        let pending = bridge
            .pending_authoring_searches
            .lock()
            .expect("pending search lock");
        assert_eq!(pending.len(), 8);
        assert_eq!(
            pending
                .iter()
                .map(|(request, _)| request.identity.clone())
                .collect::<Vec<_>>(),
            pending_identities
        );
        assert!(
            pending
                .iter()
                .all(|(request, _)| !request.cancellation.load(Ordering::Acquire))
        );
    }

    #[test]
    fn cancelled_queued_authoring_requests_terminalize_on_take_and_clear() {
        let _ = acceptance_trace::take_authoring_provider_trace_test_events();
        let bridge = DesignerIntentBridge::default();
        let identity = test_action_editor_identity(31);
        let cancellation = bridge.begin_action_search(identity.clone());
        let request = test_provider_search_request(
            identity,
            "cancel before worker",
            Arc::clone(&cancellation),
        );
        bridge
            .queue_authoring_search(request, Duration::ZERO)
            .expect("request queued before cancellation");
        cancellation.store(true, Ordering::Release);
        let (ready, next_due) = bridge.take_ready_authoring_search(Instant::now());
        assert!(ready.is_none());
        assert!(next_due.is_none());
        let take_events = acceptance_trace::take_authoring_provider_trace_test_events();
        assert_eq!(take_events.len(), 1);
        assert_eq!(take_events[0].edge, "cancelled");
        assert_eq!(take_events[0].kind, "search");
        assert_eq!(take_events[0].identity, test_action_editor_identity(31));
        assert_eq!(
            take_events[0].query_digest,
            acceptance_trace::private_trace_text_digest("cancel before worker")
        );
        assert!(
            bridge
                .pending_authoring_searches
                .lock()
                .expect("pending request lock")
                .is_empty()
        );

        let clear_identity = test_action_editor_identity(37);
        let clear_cancellation = bridge.begin_action_search(clear_identity.clone());
        let request = test_provider_search_request(
            clear_identity,
            "cancel on bridge cleanup",
            Arc::clone(&clear_cancellation),
        );
        bridge
            .queue_authoring_search(request, Duration::ZERO)
            .expect("request queued before bridge cleanup");
        bridge.clear();
        assert!(clear_cancellation.load(Ordering::Acquire));
        let clear_events = acceptance_trace::take_authoring_provider_trace_test_events();
        assert_eq!(clear_events.len(), 1);
        assert_eq!(clear_events[0].edge, "cancelled");
        assert_eq!(clear_events[0].kind, "search");
        assert_eq!(clear_events[0].identity, test_action_editor_identity(37));
        assert_eq!(
            clear_events[0].query_digest,
            acceptance_trace::private_trace_text_digest("cancel on bridge cleanup")
        );
        assert!(
            bridge
                .pending_authoring_searches
                .lock()
                .expect("pending request lock")
                .is_empty()
        );
    }

    #[test]
    fn pending_radial_add_waits_for_staged_properties_without_mutating_draft() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_ref().expect("session");
        let menu = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let cell = session.draft.menus[0].rings[0].cells[0].clone();
        let target = StableSelection::Cell {
            menu_id: menu.clone(),
            ring_id: ring.clone(),
            cell_id: cell.id.clone(),
        };
        let generation = session.generation;
        let before = std::sync::Arc::clone(&session.draft);
        editor.properties_popup = Some(target.clone());
        let mut properties = PropertiesDraft::from_cell(target, generation, &cell);
        properties.label.push_str(" staged");
        editor.properties_draft = Some(properties);
        editor.radial_add_request = Some(test_radial_add_request());

        assert!(editor.properties_popup_dirty());
        assert_eq!(
            editor.commit_pending_radial_add(
                &menu,
                &ring,
                menu::ActionInsertDestination::Cell(cell.id.clone()),
                true,
                false,
            ),
            Err(menu::MenuEditError::StaleGeneration)
        );
        let session = editor.session.as_mut().expect("session");
        assert_eq!(&*session.draft, &*before);
        assert!(!session.undo());
        assert!(editor.properties_popup_dirty());
        assert!(editor.radial_add_request.is_some());
    }

    #[test]
    fn pending_radial_add_waits_for_conflict_resolution_without_mutating_draft() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_mut().expect("session");
        let menu = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let cell = session.draft.menus[0].rings[0].cells[0].id.clone();
        let before = std::sync::Arc::clone(&session.draft);
        session.conflict = Some(crate::radial::authoring::AuthoringConflict {
            external: AuthoringSnapshot::new(std::sync::Arc::clone(&before), "external"),
            reason: "external edit requires resolution".into(),
        });
        editor.radial_add_request = Some(test_radial_add_request());

        assert_eq!(
            editor.commit_pending_radial_add(
                &menu,
                &ring,
                menu::ActionInsertDestination::Cell(cell),
                true,
                false,
            ),
            Err(menu::MenuEditError::StaleGeneration)
        );
        let session = editor.session.as_mut().expect("session");
        assert_eq!(&*session.draft, &*before);
        assert!(!session.undo());
        assert!(session.conflict.is_some());
        assert!(editor.radial_add_request.is_some());
    }

    #[test]
    fn changing_add_destination_cell_retires_replace_and_policy_consent() {
        fn frame(
            context: &egui::Context,
            editor: &mut RadialEditorState,
            events: Vec<egui::Event>,
        ) -> egui::FullOutput {
            let screen_rect =
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 720.0));
            let mut input = egui::RawInput {
                screen_rect: Some(screen_rect),
                events,
                ..Default::default()
            };
            input.viewports.insert(
                egui::ViewportId::ROOT,
                egui::ViewportInfo {
                    inner_rect: Some(screen_rect),
                    ..Default::default()
                },
            );
            context.run_ui(input, |ctx| {
                editor.radial_add_to_menu_ui(ctx, &mut RadialUiPass::default())
            })
        }

        fn widget_bounds(
            events: &[acceptance_trace::Event],
            control: &str,
            widget_part: &str,
            destination_cell_digest: u64,
        ) -> egui::Rect {
            let bounds = events
                .iter()
                .find_map(|event| match event {
                    acceptance_trace::Event::RadialInsertionControl {
                        control: observed_control,
                        widget_part: observed_part,
                        left_px,
                        top_px,
                        right_px,
                        bottom_px,
                        destination_cell_digest: observed_cell_digest,
                        visible: true,
                        fully_visible: true,
                        ..
                    } if *observed_control == control
                        && *observed_part == widget_part
                        && *observed_cell_digest == destination_cell_digest =>
                    {
                        Some([*left_px, *top_px, *right_px, *bottom_px])
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing fresh fully visible {control}/{widget_part}"));
            assert!(bounds[2] > bounds[0] && bounds[3] > bounds[1]);
            egui::Rect::from_min_max(
                egui::pos2(bounds[0] as f32, bounds[1] as f32),
                egui::pos2(bounds[2] as f32, bounds[3] as f32),
            )
        }

        let mut document = RadialDocument::starter();
        let menu = &mut document.menus[0];
        let ring = &mut menu.rings[0];
        let first = ring.cells[0].clone();
        let second = ring.cells[1].clone();
        ring.cells[0].after_action = AfterActionPolicy::KeepOpen;
        ring.cells[1].after_action = AfterActionPolicy::KeepOpen;
        let menu_id = menu.id.clone();
        let ring_id = ring.id.clone();

        let mut editor = RadialEditorState::default();
        editor.open = true;
        editor.session = Some(RadialAuthoringSession::new(AuthoringSnapshot::new(
            std::sync::Arc::new(document),
            "radial-add-destination-consent",
        )));
        editor.radial_add_request = Some(PendingRadialAdd {
            binding: crate::radial::model::ActionBinding::LauncherQuery {
                query: "close".into(),
                mode: crate::radial::model::QueryRunMode::OpenLauncher,
            },
            label: "Close window".into(),
            source_query: "close".into(),
            trace_identity: test_radial_add_trace_identity(),
        });
        editor.radial_add_menu = Some(menu_id.clone());
        editor.radial_add_ring = Some(ring_id.clone());
        editor.radial_add_cell = Some(first.id.clone());
        // These are the two approvals a user may grant for destination A.
        editor.radial_add_replace_confirmed = true;
        editor.radial_add_close_tree_confirmed = true;

        let context = egui::Context::default();
        context.enable_accesskit();
        context.set_pixels_per_point(1.0);
        acceptance_trace::reset_radial_insertion_control_test_state();
        frame(&context, &mut editor, Vec::new()).drop_without_applying_deltas();
        let trace_events = acceptance_trace::take_radial_insertion_control_test_events();
        let widget_events = trace_events
            .iter()
            .filter_map(|event| match event {
                acceptance_trace::Event::RadialInsertionControl {
                    control,
                    widget_part,
                    left_px,
                    top_px,
                    right_px,
                    bottom_px,
                    full_left_px,
                    full_top_px,
                    full_right_px,
                    full_bottom_px,
                    client_width_px,
                    client_height_px,
                    visible,
                    fully_visible,
                    ..
                } if *widget_part != "none" => Some((
                    *control,
                    *widget_part,
                    [*left_px, *top_px, *right_px, *bottom_px],
                    [*full_left_px, *full_top_px, *full_right_px, *full_bottom_px],
                    [*client_width_px, *client_height_px],
                    *visible,
                    *fully_visible,
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            widget_events
                .iter()
                .any(|(kind, part, ..)| { *kind == "destination_menu" && *part == "selector" })
        );
        assert!(
            widget_events
                .iter()
                .any(|(kind, part, ..)| { *kind == "destination_ring" && *part == "selector" })
        );
        assert!(
            widget_events
                .iter()
                .any(|(kind, part, ..)| { *kind == "destination_cell" && *part == "selector" })
        );
        assert!(
            widget_events
                .iter()
                .all(|(_, _, bounds, full, size, visible, fully)| {
                    size[0] == 1000
                        && size[1] == 720
                        && *visible
                        && bounds[0] >= 0
                        && bounds[1] >= 0
                        && bounds[2] <= size[0]
                        && bounds[3] <= size[1]
                        && (if *fully {
                            bounds == full
                        } else {
                            bounds != full
                        })
                })
        );
        let first_cell_digest = radial_add_destination_digest(Some(&first.id));
        let second_cell_digest = radial_add_destination_digest(Some(&second.id));
        let cell_combo = widget_bounds(
            &trace_events,
            "destination_cell",
            "selector",
            first_cell_digest,
        );
        frame(
            &context,
            &mut editor,
            vec![egui::Event::PointerMoved(cell_combo.center())],
        )
        .drop_without_applying_deltas();
        let _ = acceptance_trace::take_radial_insertion_control_test_events();
        frame(
            &context,
            &mut editor,
            vec![pointer_button_event(cell_combo.center(), true)],
        )
        .drop_without_applying_deltas();
        let _ = acceptance_trace::take_radial_insertion_control_test_events();
        let _popup = frame(
            &context,
            &mut editor,
            vec![pointer_button_event(cell_combo.center(), false)],
        );
        _popup.drop_without_applying_deltas();
        let popup_events = acceptance_trace::take_radial_insertion_control_test_events();
        let second_option = widget_bounds(
            &popup_events,
            "destination_cell",
            "option",
            second_cell_digest,
        );
        frame(
            &context,
            &mut editor,
            vec![egui::Event::PointerMoved(second_option.center())],
        )
        .drop_without_applying_deltas();
        frame(
            &context,
            &mut editor,
            vec![pointer_button_event(second_option.center(), true)],
        )
        .drop_without_applying_deltas();
        frame(
            &context,
            &mut editor,
            vec![pointer_button_event(second_option.center(), false)],
        )
        .drop_without_applying_deltas();

        assert_eq!(editor.radial_add_cell, Some(second.id.clone()));
        assert!(!editor.radial_add_replace_confirmed);
        assert!(!editor.radial_add_close_tree_confirmed);
        let before = std::sync::Arc::clone(&editor.session.as_ref().expect("session").draft);
        assert_eq!(
            editor.commit_pending_radial_add(
                &menu_id,
                &ring_id,
                menu::ActionInsertDestination::Cell(second.id),
                true,
                editor.radial_add_close_tree_confirmed,
            ),
            Err(menu::MenuEditError::KeepOpenIncompatible(
                crate::radial::handoff::InteractionRequirement::Deferred
            ))
        );
        let session = editor.session.as_mut().expect("session");
        assert_eq!(&*session.draft, &*before);
        assert!(!session.undo());
    }

    struct RetainedDesignerDriver {
        context: egui::Context,
        editor: RadialEditorState,
        frame: DesignerFrameContext,
    }

    impl RetainedDesignerDriver {
        fn new(editor: RadialEditorState) -> Self {
            let context = egui::Context::default();
            context.enable_accesskit();
            context.set_pixels_per_point(1.0);
            let frame = DesignerFrameContext {
                feature_defaults: crate::radial::model::RadialFeatureSettings::default(),
                expected_diagnostics: Vec::new(),
                action_catalog: std::sync::Arc::new(
                    crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot {
                        entries: Vec::new(),
                        recent_entries: Vec::new(),
                        dashboard: std::sync::Arc::new(
                            crate::dashboard::DashboardDataSnapshot::default(),
                        ),
                    },
                ),
                require_confirm_destructive: false,
                intent_bridge: std::sync::Arc::clone(&editor.intent_bridge),
                root_window_bridge: crate::visibility::RootWindowBridge::default(),
                navigation_instance: Some(1),
                jump_mode_enabled: false,
                application_input_allowed: true,
            };
            Self {
                context,
                editor,
                frame,
            }
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
            self.frame_with_modifiers(events, egui::Modifiers::default())
        }

        fn frame_with_modifiers(
            &mut self,
            events: Vec<egui::Event>,
            modifiers: egui::Modifiers,
        ) -> egui::FullOutput {
            self.frame_with_observer(events, modifiers, |_, _| {})
        }

        fn frame_with_observer(
            &mut self,
            events: Vec<egui::Event>,
            modifiers: egui::Modifiers,
            mut observe: impl FnMut(&egui::Context, &RadialEditorState),
        ) -> egui::FullOutput {
            let mut events = events;
            events.push(egui::Event::ModifiersChanged(modifiers));
            let screen_rect =
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 720.0));
            let viewport_id = radial_designer_viewport_id();
            let mut input = egui::RawInput {
                viewport_id,
                screen_rect: Some(screen_rect),
                focused: true,
                events,
                ..Default::default()
            };
            input
                .viewports
                .insert(egui::ViewportId::ROOT, egui::ViewportInfo::default());
            input.viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    focused: Some(true),
                    inner_rect: Some(screen_rect),
                    ..Default::default()
                },
            );
            let frame = &self.frame;
            let editor = &mut self.editor;
            self.context.run_ui(input, |ctx| {
                editor.viewport_ui(ctx, frame, egui::ViewportClass::Deferred);
                observe(ctx.ctx(), editor);
            })
        }

        fn frame_with_foreground_control(
            &mut self,
            mut events: Vec<egui::Event>,
            modifiers: egui::Modifiers,
        ) -> (egui::FullOutput, Option<egui::LayerId>) {
            events.push(egui::Event::ModifiersChanged(modifiers));
            let screen_rect =
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 720.0));
            let viewport_id = radial_designer_viewport_id();
            let mut input = egui::RawInput {
                viewport_id,
                screen_rect: Some(screen_rect),
                focused: true,
                events,
                ..Default::default()
            };
            input
                .viewports
                .insert(egui::ViewportId::ROOT, egui::ViewportInfo::default());
            input.viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    focused: Some(true),
                    inner_rect: Some(screen_rect),
                    ..Default::default()
                },
            );
            let frame = &self.frame;
            let editor = &mut self.editor;
            let mut focused_layer_before_editor = None;
            let output = self.context.run_ui(input, |ui| {
                egui::Area::new(egui::Id::new("radial-foreign-foreground-focus"))
                    .order(egui::Order::Foreground)
                    .show(ui.ctx(), |ui| {
                        ui.button("Foreign foreground control").request_focus();
                    });
                focused_layer_before_editor =
                    crate::gui::jump_mode::ui::focused_response_layer(ui.ctx());
                editor.viewport_ui(ui, frame, egui::ViewportClass::Deferred);
            });
            (output, focused_layer_before_editor)
        }

        fn frame_in_client(
            &mut self,
            client_size: egui::Vec2,
            time: f64,
            events: Vec<egui::Event>,
        ) -> egui::FullOutput {
            self.frame_in_client_with_observer(client_size, time, events, |_, _| {})
        }

        fn frame_in_client_with_observer(
            &mut self,
            client_size: egui::Vec2,
            time: f64,
            events: Vec<egui::Event>,
            mut observe: impl FnMut(&egui::Context, &RadialEditorState),
        ) -> egui::FullOutput {
            let client_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, client_size);
            let mut input = egui::RawInput {
                screen_rect: Some(client_rect),
                time: Some(time),
                viewport_id: radial_designer_viewport_id(),
                focused: true,
                events,
                ..Default::default()
            };
            let viewport_id = radial_designer_viewport_id();
            let viewport_rect = client_rect.translate(egui::vec2(240.0, 120.0));
            input
                .viewports
                .insert(egui::ViewportId::ROOT, egui::ViewportInfo::default());
            input.viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(egui::ViewportId::ROOT),
                    focused: Some(true),
                    inner_rect: Some(viewport_rect),
                    native_pixels_per_point: Some(1.0),
                    ..Default::default()
                },
            );
            let frame = &self.frame;
            let editor = &mut self.editor;
            self.context.run_ui(input, |ctx| {
                editor.viewport_ui(ctx, frame, egui::ViewportClass::Deferred);
                observe(ctx.ctx(), editor);
            })
        }
    }

    fn click_tree_search_result(
        driver: &mut RetainedDesignerDriver,
        query: &str,
    ) -> egui::FullOutput {
        driver.editor.tree_filter = query.into();
        let results = driver.frame(Vec::new());
        let update = results
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("retained Designer exposes AccessKit");
        let names = update
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label())
            .collect::<Vec<_>>();
        let result = update
            .nodes
            .iter()
            .find(|(_, node)| node.label().is_some_and(|name| name.contains(query)))
            .map(|(_, node)| node.bounds().expect("search result bounds"))
            .map(|bounds| {
                egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                )
            })
            .unwrap_or_else(|| panic!("missing {query:?} search result: {names:?}"));
        let point = result.center();
        results.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(point)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(point, true)])
            .drop_without_applying_deltas();
        driver.frame(vec![pointer_button_event(point, false)])
    }

    fn accesskit_named_bounds(
        output: &egui::FullOutput,
        name: &str,
        role: egui::accesskit::Role,
    ) -> egui::Rect {
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("AccessKit tree update");
        let (_, node) = update
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == role
                    && (if role == egui::accesskit::Role::Label {
                        node.value()
                    } else {
                        node.label()
                    }) == Some(name)
            })
            .unwrap_or_else(|| panic!("missing {role:?} named {name:?} in AccessKit tree"));
        let bounds = node.bounds().expect("semantic control bounds");
        egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
        )
    }

    fn accesskit_value_bounds(
        output: &egui::FullOutput,
        role: egui::accesskit::Role,
        value: &str,
    ) -> egui::Rect {
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("AccessKit tree update");
        let (_, node) = update
            .nodes
            .iter()
            .find(|(_, node)| node.role() == role && node.value() == Some(value))
            .unwrap_or_else(|| panic!("missing {role:?} with value {value:?} in AccessKit tree"));
        let bounds = node.bounds().expect("semantic control bounds");
        egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
        )
    }

    fn accesskit_value_bounds_by_vertical_order(
        output: &egui::FullOutput,
        role: egui::accesskit::Role,
        value: &str,
        order: usize,
    ) -> egui::Rect {
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("AccessKit tree update");
        let mut bounds = update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == role && node.value() == Some(value))
            .filter_map(|(_, node)| node.bounds())
            .map(|bounds| {
                egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                )
            })
            .collect::<Vec<_>>();
        bounds.sort_by(|left, right| left.top().total_cmp(&right.top()));
        *bounds
            .get(order)
            .unwrap_or_else(|| panic!("missing text input {value:?} at vertical order {order}"))
    }

    fn accesskit_any_named_bounds(output: &egui::FullOutput, name: &str) -> egui::Rect {
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("AccessKit tree update");
        let (_, node) = update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some(name))
            .unwrap_or_else(|| panic!("missing control named {name:?} in AccessKit tree"));
        let bounds = node.bounds().expect("semantic control bounds");
        egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
        )
    }

    fn accesskit_named_bounds_containing(
        output: &egui::FullOutput,
        fragment: &str,
        role: egui::accesskit::Role,
    ) -> egui::Rect {
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("AccessKit tree update");
        let (_, node) = update
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == role && node.label().is_some_and(|name| name.contains(fragment))
            })
            .unwrap_or_else(|| panic!("missing {role:?} name containing {fragment:?}"));
        let bounds = node.bounds().expect("semantic control bounds");
        egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
        )
    }

    fn accesskit_named_bounds_if_present(
        output: &egui::FullOutput,
        fragment: &str,
        role: egui::accesskit::Role,
    ) -> Option<egui::Rect> {
        accesskit_named_control_if_present(output, fragment, role).map(|(bounds, _)| bounds)
    }

    fn accesskit_named_control_if_present(
        output: &egui::FullOutput,
        fragment: &str,
        role: egui::accesskit::Role,
    ) -> Option<(egui::Rect, bool)> {
        let update = output.platform_output.accesskit_update.as_ref()?;
        let (_, node) = update.nodes.iter().find(|(_, node)| {
            node.role() == role && node.label().is_some_and(|name| name.contains(fragment))
        })?;
        let bounds = node.bounds()?;
        Some((
            egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            ),
            node.is_disabled(),
        ))
    }

    fn click_retained_button(
        driver: &mut RetainedDesignerDriver,
        _previous_output: &egui::FullOutput,
        name: &str,
    ) -> egui::FullOutput {
        // The caller's output may describe a different retained owner or a
        // pre-transition layout. Recollect the named response in the current
        // deferred viewport before deriving a real pointer position.
        let mut current = driver.frame(Vec::new());
        if driver.frame.jump_mode_enabled
            && matches!(
                driver.editor.native_jump_mode.frame_navigation_owner(),
                NavigationOwner::Blocked
            )
        {
            // A single owner-transition frame is intentionally ineligible;
            // recollect once under the candidate owner before hit-testing.
            current.drop_without_applying_deltas();
            current = driver.frame(Vec::new());
        }
        let bounds =
            accesskit_named_bounds_if_present(&current, name, egui::accesskit::Role::Button);
        let point = bounds.map(|bounds| bounds.center());
        let (jump_owner, jump_target_is_current) = if driver.frame.jump_mode_enabled {
            let owner = driver.editor.native_jump_mode.frame_navigation_owner();
            let target_is_current = point.is_some_and(|point| {
                driver
                    .editor
                    .native_jump_mode
                    .retained_registry()
                    .targets()
                    .iter()
                    .any(|target| {
                        owner.owns_surface(target.id().surface())
                            && target.metadata().kind() == crate::gui::jump_mode::TargetKind::Button
                            && target.eligible()
                            && target.clipped_rect().contains(point)
                    })
            });
            (Some(owner), target_is_current)
        } else {
            (None, true)
        };
        current.drop_without_applying_deltas();
        let bounds = bounds.unwrap_or_else(|| panic!("missing current Button named {name:?}"));
        let point = point.unwrap_or_else(|| panic!("missing current hit point for {name:?}"));
        assert!(bounds.is_positive(), "current {name:?} response is clipped");
        assert!(
            jump_target_is_current,
            "current {name:?} hit point must belong to an enabled response on exact Jump owner {:?}",
            jump_owner
        );

        let mut hover_viewport_is_child = false;
        let mut hovered_point = None;
        let hovered = driver.frame_with_observer(
            vec![egui::Event::PointerMoved(point)],
            egui::Modifiers::default(),
            |ctx, _| {
                hover_viewport_is_child = ctx.viewport_id() == radial_designer_viewport_id();
                hovered_point = ctx.input(|input| input.pointer.hover_pos());
            },
        );
        let hovered_bounds =
            accesskit_named_bounds_if_present(&hovered, name, egui::accesskit::Role::Button);
        hovered.drop_without_applying_deltas();
        assert!(hover_viewport_is_child);
        assert_eq!(hovered_point, Some(point));
        assert_eq!(
            hovered_bounds,
            Some(bounds),
            "current {name:?} response remains under the pointer"
        );

        let mut press_viewport_is_child = false;
        let mut pressed_point = None;
        let mut primary_was_pressed = false;
        let mut primary_is_down = false;
        let pressed = driver.frame_with_observer(
            vec![pointer_button_event(point, true)],
            egui::Modifiers::default(),
            |ctx, _| {
                press_viewport_is_child = ctx.viewport_id() == radial_designer_viewport_id();
                ctx.input(|input| {
                    pressed_point = input.pointer.hover_pos();
                    primary_was_pressed =
                        input.pointer.button_pressed(egui::PointerButton::Primary);
                    primary_is_down = input.pointer.primary_down();
                });
            },
        );
        let pressed_bounds =
            accesskit_named_bounds_if_present(&pressed, name, egui::accesskit::Role::Button);
        pressed.drop_without_applying_deltas();
        assert!(press_viewport_is_child);
        assert_eq!(pressed_point, Some(point));
        assert!(primary_was_pressed && primary_is_down);
        assert_eq!(
            pressed_bounds,
            Some(bounds),
            "current {name:?} response remains under the press"
        );

        driver.frame(vec![pointer_button_event(point, false)])
    }

    fn collect_retained_radial_child_owner(
        driver: &mut RetainedDesignerDriver,
        expected_kind: RadialDesignerChildKind,
    ) {
        // Opening a retained child first updates its layer, then the shared
        // navigation owner transition blocks that frame. Collect one final
        // visible frame under the exact child owner before reading its state.
        driver.frame(Vec::new()).drop_without_applying_deltas();
        driver.frame(Vec::new()).drop_without_applying_deltas();
        let mut correct_viewport = false;
        let mut child_is_active = false;
        let mut visible_layer = None;
        let owned =
            driver.frame_with_observer(Vec::new(), egui::Modifiers::default(), |ctx, editor| {
                correct_viewport = ctx.viewport_id() == radial_designer_viewport_id();
                child_is_active = editor
                    .active_radial_child_keys()
                    .iter()
                    .any(|key| key.kind == expected_kind);
                visible_layer = visible_top_middle_layer(ctx);
            });
        owned.drop_without_applying_deltas();
        assert!(correct_viewport);
        assert!(child_is_active, "the expected radial child is still active");
        let owner = driver
            .editor
            .active_radial_child_owners(1)
            .into_iter()
            .find(|owner| {
                matches!(
                    owner,
                    NavigationOwner::RadialDesignerChild { kind, .. }
                        if *kind == expected_kind
                )
            })
            .expect("the expected radial child has its native owner");
        let child_layer = RadialEditorState::radial_owner_layer(owner)
            .expect("radial child has its exact native layer");
        assert_eq!(visible_layer, Some(child_layer));
        if driver.frame.jump_mode_enabled {
            assert!(matches!(
                driver.editor.native_jump_mode.frame_navigation_owner(),
                NavigationOwner::RadialDesignerChild { kind, parent_instance: 1, .. }
                    if kind == expected_kind
            ));
        }
    }

    fn collect_retained_radial_parent_owner(driver: &mut RetainedDesignerDriver) {
        // Closing a child changes the owner candidate. The shared owner
        // transition intentionally blocks that frame, so collect its
        // transition frame before observing the stable parent owner.
        driver.frame(Vec::new()).drop_without_applying_deltas();
        let mut correct_viewport = false;
        let mut parent_is_open = false;
        let mut has_active_children = false;
        let mut visible_layer = None;
        let parent =
            driver.frame_with_observer(Vec::new(), egui::Modifiers::default(), |ctx, editor| {
                correct_viewport = ctx.viewport_id() == radial_designer_viewport_id();
                parent_is_open = editor.open;
                has_active_children = !editor.active_radial_child_keys().is_empty();
                visible_layer = visible_top_middle_layer(ctx);
            });
        parent.drop_without_applying_deltas();
        assert!(correct_viewport);
        assert!(parent_is_open, "the radial parent viewport is open");
        assert!(!has_active_children);
        assert!(
            visible_layer.is_none(),
            "no child middle window owns the retained viewport"
        );
        if driver.frame.jump_mode_enabled {
            assert_eq!(
                driver.editor.native_jump_mode.frame_navigation_owner(),
                NavigationOwner::Dialog {
                    kind: NavigationDialogKind::RadialDesigner,
                    instance: 1,
                }
            );
        }
    }

    fn click_retained_named_control(
        driver: &mut RetainedDesignerDriver,
        output: &egui::FullOutput,
        name: &str,
    ) -> egui::FullOutput {
        let point = accesskit_any_named_bounds(output, name).center();
        driver
            .frame(vec![egui::Event::PointerMoved(point)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(point, true)])
            .drop_without_applying_deltas();
        driver.frame(vec![pointer_button_event(point, false)])
    }

    fn start_retained_hint_session(
        driver: &mut RetainedDesignerDriver,
        target: &crate::gui::jump_mode::ui::RegisteredTarget,
    ) -> String {
        let leader = egui::Modifiers::SHIFT;
        driver
            .frame_with_modifiers(
                vec![
                    key_event(egui::Key::Space, leader, true),
                    egui::Event::Text(" ".into()),
                ],
                leader,
            )
            .drop_without_applying_deltas();
        driver
            .editor
            .native_jump_mode
            .active_session()
            .expect("the focused retained viewport starts a hint session")
            .hint_map()
            .hint_for(target.id())
            .expect("the current visible target is in the captured hint map")
            .as_str()
            .to_owned()
    }

    fn complete_retained_hint_session(
        driver: &mut RetainedDesignerDriver,
        hint: String,
    ) -> (egui::FullOutput, Option<egui::Id>) {
        let leader = egui::Modifiers::SHIFT;
        driver
            .frame_with_modifiers(vec![key_event(egui::Key::Space, leader, false)], leader)
            .drop_without_applying_deltas();
        let mut focused = None;
        let output = driver.frame_with_observer(
            vec![egui::Event::Text(hint)],
            egui::Modifiers::default(),
            |ctx, _| {
                assert_eq!(
                    ctx.viewport_id(),
                    radial_designer_viewport_id(),
                    "the native response is observed during the actual retained viewport pass"
                );
                focused = ctx.memory(|memory| memory.focused());
            },
        );
        (output, focused)
    }

    fn target_has_local_identity(
        target: &crate::gui::jump_mode::ui::RegisteredTarget,
        local_identity: &str,
    ) -> bool {
        let suffix = format!("{}:{local_identity}", local_identity.len());
        matches!(
            target.id().semantic(),
            crate::gui::jump_mode::SemanticTargetId::Named(identity)
                if identity.ends_with(&suffix)
        )
    }

    fn replace_retained_text(
        driver: &mut RetainedDesignerDriver,
        output: &egui::FullOutput,
        existing: &str,
        replacement: &str,
    ) -> egui::FullOutput {
        let point =
            accesskit_value_bounds(output, egui::accesskit::Role::TextInput, existing).center();
        replace_retained_text_at(driver, point, replacement)
    }

    fn replace_retained_text_at(
        driver: &mut RetainedDesignerDriver,
        point: egui::Pos2,
        replacement: &str,
    ) -> egui::FullOutput {
        driver
            .frame(vec![egui::Event::PointerMoved(point)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(point, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(point, false)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![key_event(
                egui::Key::Home,
                egui::Modifiers::default(),
                true,
            )])
            .drop_without_applying_deltas();
        driver
            .frame(vec![key_event(
                egui::Key::End,
                egui::Modifiers {
                    shift: true,
                    ..Default::default()
                },
                true,
            )])
            .drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::Text(replacement.into())])
            .drop_without_applying_deltas();
        driver.frame(Vec::new())
    }

    fn retained_properties_popup() -> (RetainedDesignerDriver, StableSelection, egui::FullOutput) {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.tree_visible = true;
        editor.tree_width = 300.0;
        editor.tree_filter = "Screen Draw".into();
        let target = {
            let session = editor.session.as_mut().expect("authoring session");
            let menu = session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == MenuId::new("starter-screen-tools"))
                .expect("screen tools menu");
            let ring = &menu.rings[0];
            let cell = ring
                .cells
                .iter()
                .find(|cell| cell.id == CellId::new("starter-screen-tools-draw"))
                .expect("screen draw action");
            let target = StableSelection::Cell {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: cell.id.clone(),
            };
            session.select(Some(target.clone()));
            target
        };
        editor.properties_popup = Some(target.clone());
        let mut driver = RetainedDesignerDriver::new(editor);
        let initial = driver.frame(Vec::new());
        assert!(driver.editor.properties_draft.is_some());
        (driver, target, initial)
    }

    fn retained_jump_properties_popup() -> (RetainedDesignerDriver, StableSelection) {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.tree_visible = true;
        editor.tree_width = 300.0;
        if let Some(session) = editor.session.as_mut() {
            // This focused Jump fixture does not provide an authoring client.
            // Mark the optional catalog as already settled so the first real
            // callback cannot introduce an unrelated ServiceClosed status.
            session.font_catalog_loaded = true;
        }
        let target = {
            let session = editor.session.as_mut().expect("authoring session");
            let menu = session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == MenuId::new("starter-screen-tools"))
                .expect("screen tools menu");
            let ring = &menu.rings[0];
            let cell = ring
                .cells
                .iter()
                .find(|cell| cell.id == CellId::new("starter-screen-tools-draw"))
                .expect("screen draw action");
            let target = StableSelection::Cell {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: cell.id.clone(),
            };
            session.select(Some(target.clone()));
            target
        };
        editor.properties_popup = Some(target.clone());
        let mut driver = RetainedDesignerDriver::new(editor);
        driver.frame.jump_mode_enabled = true;

        let opening = driver.frame(Vec::new());
        opening.drop_without_applying_deltas();
        let stable = driver.frame(Vec::new());
        stable.drop_without_applying_deltas();
        // The first frame that observes the new child layer is intentionally
        // blocked by the shared owner-transition guard. Collect once more
        // under the now-stable exact child owner before inspecting targets.
        let child_owned = driver.frame(Vec::new());
        child_owned.drop_without_applying_deltas();
        assert!(matches!(
            driver.editor.native_jump_mode.frame_navigation_owner(),
            NavigationOwner::RadialDesignerChild {
                kind: RadialDesignerChildKind::CellProperties,
                parent_instance: 1,
                ..
            }
        ));
        (driver, target)
    }

    fn retained_jump_query_properties_popup()
    -> (RetainedDesignerDriver, StableSelection, egui::FullOutput) {
        let (mut driver, target) = retained_jump_properties_popup();
        let binding = ActionBinding::LauncherQuery {
            query: String::new(),
            mode: crate::radial::model::QueryRunMode::OpenLauncher,
        };
        let session = driver.editor.session.as_ref().expect("authoring session");
        let scope = BindingEditorScope {
            surface: BindingEditorSurface::Properties,
            editor_session: session.editor_session,
            draft_generation: session.generation,
            target: target.clone(),
            slot: BindingEditorSlot::CellPrimary,
        };
        let draft = driver
            .editor
            .properties_draft
            .as_mut()
            .expect("retained Properties draft");
        draft.action_binding = Some(binding.clone());
        draft
            .action_editor
            .seed_unassigned_query_for_test(scope, Some(&binding), "");
        let rendered = driver.frame(Vec::new());
        (driver, target, rendered)
    }

    fn retained_jump_radial_add_request() -> RetainedDesignerDriver {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.radial_add_request = Some(test_radial_add_request());
        if let Some(session) = editor.session.as_mut() {
            session.font_catalog_loaded = true;
        }
        let mut driver = RetainedDesignerDriver::new(editor);
        driver.frame.jump_mode_enabled = true;
        let opening = driver.frame(Vec::new());
        opening.drop_without_applying_deltas();
        let stable = driver.frame(Vec::new());
        stable.drop_without_applying_deltas();
        // The first frame that observes the new child layer is intentionally
        // blocked by the shared owner-transition guard. Collect once more
        // under the now-stable exact child owner before inspecting targets.
        let child_owned = driver.frame(Vec::new());
        child_owned.drop_without_applying_deltas();
        assert!(matches!(
            driver.editor.native_jump_mode.frame_navigation_owner(),
            NavigationOwner::RadialDesignerChild {
                kind: RadialDesignerChildKind::AddToRadial,
                parent_instance: 1,
                subject: 901,
                ..
            }
        ));
        driver
    }

    fn retained_inspector_editor() -> (RadialEditorState, StableSelection) {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.inspector_visible = true;
        editor.inspector_width = 320.0;
        let target = {
            let session = editor.session.as_mut().expect("authoring session");
            let menu = session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == MenuId::new("starter-screen-tools"))
                .expect("screen tools menu");
            let ring = &menu.rings[0];
            let cell = ring
                .cells
                .iter()
                .find(|cell| cell.id == CellId::new("starter-screen-tools-draw"))
                .expect("screen draw action");
            let target = StableSelection::Cell {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: cell.id.clone(),
            };
            session.select(Some(target.clone()));
            target
        };
        (editor, target)
    }

    fn retained_bulk_style_driver(field: &str) -> RetainedDesignerDriver {
        use crate::radial::authoring::{AuthoredCellTarget, AuthoringSnapshot};

        let mut document = RadialDocument::starter();
        let menu = &mut document.menus[0];
        let ring = &mut menu.rings[0];
        ring.cells[0].style.text.bold = crate::radial::model::Override::Value(false);
        ring.cells[1].style.text.bold = crate::radial::model::Override::Inherit;
        ring.cells[0].style.text.font_size = crate::radial::model::Override::Value(12.0);
        ring.cells[1].style.text.font_size = crate::radial::model::Override::Value(24.0);
        let targets = ring.cells[..2]
            .iter()
            .map(|cell| AuthoredCellTarget {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: cell.id.clone(),
            })
            .collect::<Vec<_>>();
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let mut session = RadialAuthoringSession::new(AuthoringSnapshot::new(
            std::sync::Arc::new(document),
            "retained-bulk-style",
        ));
        session.select_authored_cell(targets[0].clone(), false, false);
        session.select_authored_cell(targets[1].clone(), true, false);
        editor.session = Some(session);
        editor.inspector_visible = true;
        editor.inspector_width = 360.0;
        editor.tree_visible = false;
        editor.bulk_style_field = field.into();
        RetainedDesignerDriver::new(editor)
    }

    fn assert_retained_bulk_style_applied(
        driver: &mut RetainedDesignerDriver,
        before: &RadialDocument,
        expected: &RadialDocument,
        generation: crate::radial::authoring::DraftGeneration,
        apply_name: &str,
    ) {
        let selection = driver.editor.session.as_ref().unwrap().selection.clone();
        let controls = driver.frame(Vec::new());
        assert_eq!(*driver.editor.session.as_ref().unwrap().draft, *before);
        assert_eq!(
            driver.editor.session.as_ref().unwrap().generation,
            generation
        );
        let applied = click_retained_button(driver, &controls, apply_name);
        controls.drop_without_applying_deltas();
        applied.drop_without_applying_deltas();
        driver.frame(Vec::new()).drop_without_applying_deltas();
        let session = driver.editor.session.as_mut().unwrap();
        assert_eq!(*session.draft, *expected);
        assert_eq!(session.generation.0, generation.0 + 1);
        assert_eq!(session.acceptance_history_depths(), (1, 0));
        assert_eq!(session.selection, selection);
        assert!(session.undo());
        assert_eq!(*session.draft, *before);
        assert_eq!(session.selection, selection);
        assert!(session.redo());
        assert_eq!(*session.draft, *expected);
        assert_eq!(session.selection, selection);
    }

    #[test]
    fn retained_bulk_checkbox_stays_staged_across_frames_and_apply_focus_loss() {
        let mut driver = retained_bulk_style_driver("text.bold");
        let before = (*driver.editor.session.as_ref().unwrap().draft).clone();
        let generation = driver.editor.session.as_ref().unwrap().generation;
        let first = driver.frame(Vec::new());
        click_retained_named_control(&mut driver, &first, "text.bold")
            .drop_without_applying_deltas();
        first.drop_without_applying_deltas();
        for _ in 0..3 {
            let output = driver.frame(Vec::new());
            assert!(
                output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .any(|(_, node)| {
                        node.label() == Some("text.bold")
                            && node.toggled() == Some(egui::accesskit::Toggled::True)
                    })
            );
            assert_eq!(
                driver.editor.bulk_style_staged_value,
                Some(skin_editor::with_payload(serde_json::json!(true)))
            );
            output.drop_without_applying_deltas();
        }
        let mut expected = before.clone();
        for cell in &mut expected.menus[0].rings[0].cells[..2] {
            cell.style.text.bold = crate::radial::model::Override::Value(true);
        }
        assert_retained_bulk_style_applied(
            &mut driver,
            &before,
            &expected,
            generation,
            "Apply text.bold to 2 cells",
        );
    }

    #[test]
    fn retained_bulk_scalar_stays_staged_across_frames_and_apply_focus_loss() {
        let mut driver = retained_bulk_style_driver("text.font_size");
        let before = (*driver.editor.session.as_ref().unwrap().draft).clone();
        let generation = driver.editor.session.as_ref().unwrap().generation;
        let first = driver.frame(Vec::new());
        let point = accesskit_any_named_bounds(&first, "text.font_size").center();
        first.drop_without_applying_deltas();
        replace_retained_text_at(&mut driver, point, "38.5").drop_without_applying_deltas();
        // Commit the local DragValue text edit before checking retained frames.
        driver
            .frame(vec![key_event(
                egui::Key::Enter,
                egui::Modifiers::default(),
                true,
            )])
            .drop_without_applying_deltas();
        let staged = skin_editor::with_payload(serde_json::json!(38.5));
        for _ in 0..3 {
            driver.frame(Vec::new()).drop_without_applying_deltas();
            assert_eq!(driver.editor.bulk_style_staged_value, Some(staged.clone()));
        }
        let mut expected = before.clone();
        for cell in &mut expected.menus[0].rings[0].cells[..2] {
            cell.style.text.font_size = crate::radial::model::Override::Value(38.5);
        }
        assert_retained_bulk_style_applied(
            &mut driver,
            &before,
            &expected,
            generation,
            "Apply text.font_size to 2 cells",
        );
    }

    fn seed_inspector_query(
        editor: &mut RadialEditorState,
        target: &StableSelection,
        query: &str,
    ) -> ActionBinding {
        let (scope, binding) = {
            let session = editor.session.as_ref().expect("authoring session");
            let StableSelection::Cell {
                menu_id,
                ring_id,
                cell_id,
            } = target
            else {
                panic!("Inspector fixture must target a cell")
            };
            let binding = match &authored_cell(editor, menu_id, ring_id, cell_id).content {
                CellContent::Action { binding } => binding.clone(),
                content => panic!("Inspector fixture cell must be an action: {content:?}"),
            };
            let scope = BindingEditorScope {
                surface: BindingEditorSurface::Inspector,
                editor_session: session.editor_session,
                draft_generation: session.generation,
                target: target.clone(),
                slot: BindingEditorSlot::CellPrimary,
            };
            (scope, binding)
        };
        editor.inspector_action_editor_scope = Some(scope.clone());
        editor
            .inspector_action_editor
            .seed_unassigned_query_for_test(scope, Some(&binding), query);
        binding
    }

    fn retained_dirty_inspector(
        query: &str,
    ) -> (
        RetainedDesignerDriver,
        StableSelection,
        ActionBinding,
        egui::FullOutput,
    ) {
        let (mut editor, target) = retained_inspector_editor();
        let binding = seed_inspector_query(&mut editor, &target, query);
        let mut driver = RetainedDesignerDriver::new(editor);
        let initial = driver.frame(Vec::new());
        (driver, target, binding, initial)
    }

    fn queue_navigation_to_screenshot_region(driver: &mut RetainedDesignerDriver) {
        let target = {
            let session = driver.editor.session.as_ref().expect("authoring session");
            let menu = session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == MenuId::new("starter-screen-tools"))
                .expect("screen tools menu");
            let ring = menu
                .rings
                .iter()
                .find(|ring| {
                    ring.cells
                        .iter()
                        .any(|cell| cell.id == CellId::new("starter-screen-tools-screenshot"))
                })
                .expect("screen tools ring");
            let cell = ring
                .cells
                .iter()
                .find(|cell| cell.id == CellId::new("starter-screen-tools-screenshot"))
                .expect("screenshot region cell");
            StableSelection::Cell {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: cell.id.clone(),
            }
        };
        driver
            .editor
            .queue_navigation_intent(DesignerNavigationIntent::Select {
                target,
                control: false,
                shift: false,
                force_history: false,
                open_properties: false,
            });
    }

    fn test_pinned_binding() -> ActionBinding {
        ActionBinding::Persisted {
            action: crate::universal_actions::PersistedUniversalActionRef {
                target: Some(
                    crate::universal_actions::PersistableActionTargetRef::LegacyAction {
                        action: crate::actions::Action {
                            label: "Screenshot region".into(),
                            desc: "Retained Properties Pin fixture".into(),
                            action: "screenshot:region".into(),
                            args: None,
                        },
                    },
                ),
                action_id: crate::universal_actions::action_ids::RESULT_EXECUTE,
            },
        }
    }

    fn cell_target_parts(target: &StableSelection) -> (MenuId, RingId, CellId) {
        match target {
            StableSelection::Cell {
                menu_id,
                ring_id,
                cell_id,
            } => (menu_id.clone(), ring_id.clone(), cell_id.clone()),
            _ => unreachable!("Properties popup is scoped to a cell"),
        }
    }

    fn seed_pinned_properties_draft(
        driver: &mut RetainedDesignerDriver,
        target: &StableSelection,
        binding: ActionBinding,
        query: &str,
    ) {
        let (menu_id, ring_id, cell_id) = cell_target_parts(target);
        let (editor_session, generation) = {
            let session = driver.editor.session.as_ref().expect("authoring session");
            (session.editor_session, session.generation)
        };
        driver
            .editor
            .properties_draft
            .as_mut()
            .expect("Properties draft initialized by retained frame")
            .action_binding = Some(binding.clone());
        driver
            .editor
            .properties_draft
            .as_mut()
            .unwrap()
            .action_editor
            .seed_pinned_search_for_test(
                BindingEditorScope {
                    surface: BindingEditorSurface::Properties,
                    editor_session,
                    draft_generation: generation,
                    target: target.clone(),
                    slot: BindingEditorSlot::CellPrimary,
                },
                binding.clone(),
                query,
            );
        menu::set_cell_content(
            driver.editor.session.as_mut().expect("authoring session"),
            &menu_id,
            &ring_id,
            &cell_id,
            CellContent::Action {
                binding: binding.clone(),
            },
        )
        .expect("fixture cell accepts the pinned action");
        let generation = driver.editor.session.as_ref().unwrap().generation;
        driver.editor.properties_draft.as_mut().unwrap().generation = generation;
    }

    fn authored_cell<'a>(
        editor: &'a RadialEditorState,
        menu_id: &MenuId,
        ring_id: &RingId,
        cell_id: &CellId,
    ) -> &'a crate::radial::model::CellDefinition {
        editor
            .session
            .as_ref()
            .expect("authoring session")
            .draft
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
            .and_then(|ring| ring.cells.iter().find(|cell| &cell.id == cell_id))
            .expect("authored cell")
    }

    fn key_event(key: egui::Key, modifiers: egui::Modifiers, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        }
    }

    fn pointer_button_event(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    #[test]
    fn default_designer_layout_is_canvas_first_and_panes_fit_compact_windows() {
        let preferences = crate::settings::RadialDesignerPreferences::default();
        assert!(!preferences.tree_visible);
        assert!(!preferences.inspector_visible);
        let canvas_only = designer_pane_layout(
            egui::vec2(900.0, 650.0),
            egui::vec2(8.0, 4.0),
            preferences.tree_visible,
            preferences.inspector_visible,
            preferences.tree_width,
            preferences.inspector_width,
        );
        assert!(canvas_only.canvas_width >= 899.0);
        let compact = designer_pane_layout(
            egui::vec2(700.0, 470.0),
            egui::vec2(8.0, 4.0),
            true,
            true,
            preferences.tree_width,
            preferences.inspector_width,
        );
        assert!(compact.canvas_width > 0.0);
        assert!(compact.height + 4.0 <= 470.0);
        assert!(
            compact.tree_width + compact.canvas_width + compact.inspector_width + 28.0 <= 700.01
        );
        let one_pane = designer_pane_layout(
            egui::vec2(700.0, 470.0),
            egui::vec2(8.0, 4.0),
            true,
            false,
            preferences.tree_width,
            preferences.inspector_width,
        );
        assert!(one_pane.tree_width + one_pane.canvas_width + 14.0 <= 700.01);
    }

    #[test]
    fn compact_designer_pane_reserves_trailing_spacing_and_client_rounding_guard() {
        let preferences = crate::settings::RadialDesignerPreferences::default();
        let layout = designer_pane_layout(
            egui::vec2(624.0, 299.0),
            egui::vec2(8.0, 4.0),
            true,
            true,
            preferences.tree_width,
            preferences.inspector_width,
        );

        assert_eq!(layout.height, 291.0);
        assert!(layout.height + 8.0 <= 299.0);
    }

    #[test]
    fn designer_pane_width_budget_includes_actual_horizontal_item_spacing() {
        for horizontal_spacing in [0.0, 8.0, 17.0] {
            for (tree_visible, inspector_visible) in [(true, true), (true, false), (false, true)] {
                let layout = designer_pane_layout(
                    egui::vec2(704.0, 460.0),
                    egui::vec2(horizontal_spacing, 4.0),
                    tree_visible,
                    inspector_visible,
                    180.0,
                    300.0,
                );
                let gap_count = usize::from(tree_visible) + usize::from(inspector_visible);
                let actual_width = layout.tree_width
                    + layout.canvas_width
                    + layout.inspector_width
                    + (6.0 + horizontal_spacing) * gap_count as f32;
                assert!((actual_width - 704.0).abs() < 0.01);
                assert!(layout.canvas_width >= 140.0);
            }
        }
    }

    #[test]
    fn retained_designer_both_panes_fit_normal_and_compact_client_geometry() {
        for client_size in [egui::vec2(900.0, 720.0), egui::vec2(720.0, 520.0)] {
            let (mut editor, _) = retained_inspector_editor();
            editor.tree_visible = true;
            editor.inspector_visible = true;
            editor.tree_width = 180.0;
            editor.inspector_width = 300.0;
            let session = editor.session.as_ref().unwrap();
            let before = (
                session.draft.clone(),
                session.generation,
                session.selection.clone(),
                session.acceptance_history_depths(),
                session.pending_assets.clone(),
            );
            let mut driver = RetainedDesignerDriver::new(editor);
            let _ = acceptance_trace::take_action_editor_control_test_events();
            driver
                .frame_in_client(client_size, 0.0, Vec::new())
                .drop_without_applying_deltas();
            let output = driver.frame_in_client(client_size, 0.05, Vec::new());
            let tree_inputs = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter(|(_, node)| {
                    node.role() == egui::accesskit::Role::TextInput
                        && node.placeholder() == Some("Authored menus, labels, or IDs")
                        && node.value() == Some("")
                })
                .collect::<Vec<_>>();
            assert_eq!(tree_inputs.len(), 1, "uniquely identify Tree Search");
            let bounds = tree_inputs[0].1.bounds().unwrap();
            let search = egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            );
            assert!(search.is_positive());
            assert!(
                search.right() < client_size.x / 2.0,
                "Tree Search remains in its left pane"
            );
            let inspector_heading =
                accesskit_named_bounds(&output, "Inspector", egui::accesskit::Role::Label);
            let scroll_origin = egui::pos2(
                inspector_heading.center().x,
                inspector_heading.bottom() + 20.0,
            );
            output.drop_without_applying_deltas();
            let mut query_geometry = None;
            for step in 0..12 {
                let events = if step == 0 {
                    Vec::new()
                } else {
                    vec![
                        egui::Event::PointerMoved(scroll_origin),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -40.0),
                            phase: egui::TouchPhase::Move,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                };
                let _ = acceptance_trace::take_action_editor_control_test_events();
                driver
                    .frame_in_client(client_size, 0.1 + step as f64 * 0.2, events)
                    .drop_without_applying_deltas();
                let controls = acceptance_trace::take_action_editor_control_test_events();
                for event in controls {
                    if let Event::DesignerActionEditorControl {
                        surface: "inspector",
                        control: "query_field",
                        left_px,
                        top_px,
                        right_px,
                        bottom_px,
                        full_left_px,
                        full_top_px,
                        full_right_px,
                        full_bottom_px,
                        client_width_px,
                        client_height_px,
                        fully_visible,
                        ..
                    } = event
                    {
                        let full = [full_left_px, full_top_px, full_right_px, full_bottom_px];
                        let clipped = [left_px, top_px, right_px, bottom_px];
                        assert_eq!(
                            [client_width_px, client_height_px],
                            [client_size.x as i32, client_size.y as i32]
                        );
                        assert_eq!(
                            left_px, full_left_px,
                            "left pane clip at {client_size:?}: {full:?} vs {clipped:?}"
                        );
                        assert_eq!(
                            right_px, full_right_px,
                            "right pane clip at {client_size:?}: {full:?} vs {clipped:?}"
                        );
                        assert!(full_left_px as f32 > search.right());
                        assert!(
                            full_right_px <= client_width_px,
                            "field exceeds the actual client: {full:?}"
                        );
                        if fully_visible {
                            assert_eq!(clipped, full);
                            assert!(full_top_px >= 0 && full_bottom_px <= client_height_px);
                            query_geometry = Some(full);
                        }
                    }
                }
                if query_geometry.is_some() {
                    break;
                }
            }
            assert!(
                query_geometry.is_some(),
                "the actual Inspector query field is fully reachable at {client_size:?}"
            );
            let session = driver.editor.session.as_ref().unwrap();
            assert_eq!(
                (
                    session.draft.clone(),
                    session.generation,
                    session.selection.clone(),
                    session.acceptance_history_depths(),
                    session.pending_assets.clone(),
                ),
                before,
            );
        }
    }

    #[test]
    fn resetting_designer_layout_does_not_touch_dirty_document_or_selection() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.make_dirty_for_test();
        let selection = StableSelection::Menu(
            editor
                .session
                .as_ref()
                .expect("session")
                .draft
                .default_menu_id
                .clone(),
        );
        editor
            .session
            .as_mut()
            .expect("session")
            .select(Some(selection.clone()));
        let before_draft = editor.session.as_ref().unwrap().draft.clone();
        let before_generation = editor.session.as_ref().unwrap().generation;
        editor.tree_visible = true;
        editor.inspector_visible = true;
        editor.tree_width = 410.0;
        editor.inspector_width = 540.0;
        editor.preview_zoom = 1.8;
        editor.canvas_pan = CanvasPoint::new(22.0, -11.0);
        editor
            .preferences
            .expanded_sections
            .insert("menu:starter".into(), true);
        let tree_epoch = editor.tree_widget_epoch;
        editor.reset_designer_layout();
        let session = editor.session.as_ref().unwrap();
        assert_eq!(session.draft, before_draft);
        assert_eq!(session.generation, before_generation);
        assert_eq!(session.selection, Some(selection));
        assert!(!editor.tree_visible);
        assert!(!editor.inspector_visible);
        assert_eq!(editor.preview_zoom, 1.0);
        assert_eq!(editor.canvas_pan, CanvasPoint::default());
        assert!(editor.preferences.expanded_sections.is_empty());
        assert_ne!(editor.tree_widget_epoch, tree_epoch);
        assert!(editor.preferences_dirty);
    }

    #[test]
    fn designer_viewport_identity_and_duplicate_open_reuse_one_draft() {
        assert_eq!(radial_designer_viewport_id(), radial_designer_viewport_id());
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.make_dirty_for_test();
        let generation = editor.session.as_ref().unwrap().generation;
        editor.open();
        assert_eq!(editor.session.as_ref().unwrap().generation, generation);
        assert!(editor.is_dirty());
        editor.open_skins();
        assert!(editor.is_showing_resources());
    }

    #[test]
    fn retained_inspector_keep_editing_preserves_buffers_and_discard_tears_down_owner() {
        let (editor, target) = retained_inspector_editor();
        let session = editor.session.as_ref().unwrap();
        let before = (
            session.draft.clone(),
            session.editor_session,
            session.generation,
            session.selection.clone(),
            session.acceptance_history_depths(),
            session.pending_assets.clone(),
        );
        let mut driver = RetainedDesignerDriver::new(editor);
        let initial = driver.frame(Vec::new());
        let query = "unassigned Inspector close fixture";
        replace_retained_text(&mut driver, &initial, "", query).drop_without_applying_deltas();
        initial.drop_without_applying_deltas();
        assert_eq!(
            driver.editor.inspector_action_editor.query_for_test(),
            query
        );
        assert!(driver.editor.properties_popup_dirty());
        let identity = driver.editor.inspector_action_editor.identity().unwrap();
        assert!(driver.editor.action_editor_request_is_current(&identity));

        driver.editor.request_close();
        assert!(driver.editor.close_prompt);
        assert!(!driver.editor.action_editor_request_is_current(&identity));
        let prompt = driver.frame(Vec::new());
        let keep_editing = click_retained_button(&mut driver, &prompt, "Keep editing");
        prompt.drop_without_applying_deltas();
        keep_editing.drop_without_applying_deltas();
        collect_retained_radial_parent_owner(&mut driver);
        assert!(driver.editor.open);
        assert!(!driver.editor.close_prompt);
        assert_eq!(
            driver.editor.inspector_action_editor.query_for_test(),
            query
        );
        assert!(driver.editor.properties_popup_dirty());
        assert!(!driver.editor.action_editor_request_is_current(&identity));
        let current_identity = driver.editor.inspector_action_editor.identity().unwrap();
        assert_ne!(current_identity.editor_epoch, identity.editor_epoch);
        assert!(
            driver
                .editor
                .action_editor_request_is_current(&current_identity)
        );
        let session = driver.editor.session.as_ref().unwrap();
        assert_eq!(
            (
                session.draft.clone(),
                session.editor_session,
                session.generation,
                session.selection.clone(),
                session.acceptance_history_depths(),
                session.pending_assets.clone(),
            ),
            before,
        );

        driver.editor.request_close();
        let prompt = driver.frame(Vec::new());
        let discarded = click_retained_button(&mut driver, &prompt, "Discard");
        prompt.drop_without_applying_deltas();
        discarded.drop_without_applying_deltas();
        let closed = driver.editor.acceptance_observation().unwrap();
        assert!(!closed.open && !closed.properties_dirty && !closed.properties_popup_open);
        assert!(closed.action_editor.is_none());
        assert_eq!((closed.session_id, closed.generation), (0, 0));
        assert!(!closed.initial_snapshot_pending);
        assert!(driver.editor.session.is_none());
        assert!(driver.editor.inspector_action_editor_scope.is_none());
        assert_eq!(driver.editor.inspector_action_editor.query_for_test(), "");
        assert!(!driver.editor.inspector_action_editor.has_unassigned_text());
        assert!(driver.editor.inspector_action_editor.identity().is_none());
        assert!(driver.editor.viewport_close_pending);
        assert!(
            !driver
                .editor
                .action_editor_request_is_current(&current_identity)
        );

        driver.editor.open_test_snapshot();
        driver.editor.session.as_mut().unwrap().select(Some(target));
        collect_retained_radial_parent_owner(&mut driver);
        let reopened = driver.editor.acceptance_observation().unwrap();
        assert!(reopened.open);
        assert!(!reopened.properties_dirty && !reopened.draft_dirty);
        assert_eq!(driver.editor.session.as_ref().unwrap().draft, before.0);
        assert_ne!(
            driver.editor.session.as_ref().unwrap().editor_session,
            before.1
        );
        assert_eq!(driver.editor.inspector_action_editor.query_for_test(), "");
        assert!(
            !driver
                .editor
                .action_editor_request_is_current(&current_identity)
        );
        assert_eq!((reopened.undo_depth, reopened.redo_depth), (0, 0));
        assert!(
            reopened
                .action_editor
                .is_some_and(|editor| !editor.search_pending && !editor.test_pending)
        );
    }

    #[test]
    fn force_close_disposes_unassigned_inspector_buffers_and_observation() {
        let (mut driver, _, _, initial) = retained_dirty_inspector("force-close local draft");
        initial.drop_without_applying_deltas();
        let identity = driver.editor.inspector_action_editor.identity().unwrap();
        assert!(driver.editor.properties_popup_dirty());
        driver.editor.force_close();
        let closed = driver.editor.acceptance_observation().unwrap();
        assert!(!closed.open && !closed.properties_dirty && !closed.properties_popup_open);
        assert!(closed.action_editor.is_none());
        assert_eq!((closed.session_id, closed.generation), (0, 0));
        assert!(!closed.initial_snapshot_pending);
        assert!(driver.editor.session.is_none());
        assert!(driver.editor.inspector_action_editor_scope.is_none());
        assert_eq!(driver.editor.inspector_action_editor.query_for_test(), "");
        assert!(!driver.editor.inspector_action_editor.has_unassigned_text());
        assert!(!driver.editor.action_editor_request_is_current(&identity));
        assert!(driver.editor.viewport_close_pending);
    }

    #[test]
    fn cancelling_center_placement_keeps_the_authoring_document_unchanged() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let (menu_id, ring_id, cell_id, generation, before) = {
            let session = editor.session.as_ref().unwrap();
            (
                session.draft.menus[0].id.clone(),
                session.draft.menus[0].rings[0].id.clone(),
                session.draft.menus[0].rings[0].cells[0].id.clone(),
                session.generation,
                session.draft.clone(),
            )
        };
        editor.placement_draft = Some(PlacementDraft::new(menu_id, ring_id, cell_id, generation));
        editor.placement_draft = None;
        assert_eq!(editor.session.as_ref().unwrap().draft, before);
    }

    #[test]
    fn closing_during_preview_preparation_disposes_the_pending_viewport_state() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_mut().unwrap();
        session.pending_request = Some(crate::radial::authoring::PendingAuthoringRequest {
            id: crate::radial::authoring::AuthoringRequestId(42),
            generation: session.generation,
            editor_session: session.editor_session,
            kind: crate::radial::authoring::PendingRequestKind::PrepareEmbeddedPreview,
        });

        editor.request_close();

        assert!(!editor.open);
        assert!(editor.session.is_none());
        assert!(editor.viewport_close_pending);
    }

    #[test]
    fn closed_show_deferred_frames_skip_catalog_and_real_retained_callback_drains_close() {
        use std::sync::atomic::Ordering;

        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let app = radial_viewport_test_app(&ctx, workspace.root());
        let shared = Arc::clone(&app.radial_editor);
        let viewport_id = radial_designer_viewport_id();
        let root_frame = || {
            ctx.run_ui(egui::RawInput::default(), |root| {
                RadialEditorState::show_deferred(&shared, root, &app, None, false, true);
            })
        };
        let initial_builds = app.authoring_catalog_build_count.load(Ordering::SeqCst);
        for _ in 0..32 {
            let output = root_frame();
            assert!(!output.viewport_output.contains_key(&viewport_id));
            assert_eq!(
                app.authoring_catalog_build_count.load(Ordering::SeqCst),
                initial_builds
            );
            output.drop_without_applying_deltas();
        }

        shared.lock().unwrap().open_test_snapshot();
        let opened = root_frame();
        assert!(
            opened.viewport_output[&viewport_id]
                .viewport_ui_cb
                .is_some()
        );
        assert_eq!(
            app.authoring_catalog_build_count.load(Ordering::SeqCst),
            initial_builds + 1
        );
        opened.drop_without_applying_deltas();
        for _ in 0..8 {
            let output = root_frame();
            assert!(output.viewport_output.contains_key(&viewport_id));
            output.drop_without_applying_deltas();
        }
        assert_eq!(
            app.authoring_catalog_build_count.load(Ordering::SeqCst),
            initial_builds + 1,
            "unchanged open ROOT frames reuse the actual provider snapshot"
        );
        {
            let mut editor = shared.lock().unwrap();
            let session = editor.session.as_mut().unwrap();
            session.pending_request = Some(crate::radial::authoring::PendingAuthoringRequest {
                id: crate::radial::authoring::AuthoringRequestId(42),
                generation: session.generation,
                editor_session: session.editor_session,
                kind: crate::radial::authoring::PendingRequestKind::PrepareEmbeddedPreview,
            });
            editor.request_close();
            assert!(!editor.open);
            assert!(editor.session.is_none());
            assert!(editor.viewport_close_pending);
        }
        app.root_window_bridge.set_designer_identity_for_test(23);
        let prior_identity = app.root_window_bridge.designer_identity_for_test();
        let closing = root_frame();
        let callback = closing.viewport_output[&viewport_id]
            .viewport_ui_cb
            .clone()
            .expect("the real closing deferred callback is retained by egui");
        assert!(shared.lock().unwrap().viewport_close_pending);
        assert_eq!(
            app.authoring_catalog_build_count.load(Ordering::SeqCst),
            initial_builds + 1
        );
        closing.drop_without_applying_deltas();
        let mut child_input = egui::RawInput {
            viewport_id,
            ..Default::default()
        };
        child_input.viewports.insert(
            viewport_id,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                ..Default::default()
            },
        );
        let initial_child_output = ctx.run_ui(child_input.clone(), |_| {});
        let initial_child_commands = &initial_child_output.viewport_output[&viewport_id].commands;
        assert!(
            matches!(
                initial_child_commands.as_slice(),
                [egui::ViewportCommand::SetTheme(_)]
            ),
            "{:?}",
            initial_child_commands
        );
        assert!(shared.lock().unwrap().viewport_close_pending);
        initial_child_output.drop_without_applying_deltas();

        let drained = ctx.run_ui(child_input, |child| callback(child));
        {
            let editor = shared.lock().unwrap();
            assert!(!editor.open);
            assert!(!editor.viewport_close_pending);
            assert!(editor.session.is_none());
        }
        let cleared_identity = app.root_window_bridge.designer_identity_for_test();
        assert_eq!(cleared_identity.0, 0);
        assert!(cleared_identity.1 > prior_identity.1);
        assert!(drained.viewport_output[&viewport_id].commands.is_empty());
        drained.drop_without_applying_deltas();
        for _ in 0..32 {
            let output = root_frame();
            assert!(!output.viewport_output.contains_key(&viewport_id));
            assert_eq!(output.viewport_output.len(), 1);
            assert_eq!(
                app.authoring_catalog_build_count.load(Ordering::SeqCst),
                initial_builds + 1
            );
            let editor = shared.lock().unwrap();
            assert!(!editor.viewport_close_pending);
            assert!(editor.session.is_none());
            assert_eq!(
                app.root_window_bridge.designer_identity_for_test(),
                cleared_identity
            );
            drop(editor);
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn closing_pending_native_preview_supersedes_with_terminal_stop() {
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.client = Some(client);
        let start = {
            let session = editor.session.as_mut().unwrap();
            session
                .request_start_native_preview(session.draft.default_menu_id.clone(), false, None)
                .unwrap()
        };
        editor.client.as_ref().unwrap().send(start).unwrap();

        editor.request_close();

        let queued_start = endpoint.request_rx.try_recv().expect("start request");
        let queued_stop = endpoint.request_rx.try_recv().expect("stop request");
        assert!(matches!(
            queued_start,
            crate::radial::authoring::AuthoringRequest::StartNativePreview { .. }
        ));
        assert!(matches!(
            queued_stop,
            crate::radial::authoring::AuthoringRequest::StopNativePreview { .. }
        ));
        assert!(editor.open);
        assert_eq!(
            editor
                .session
                .as_ref()
                .and_then(|session| session.pending_native_preview)
                .map(|pending| pending.kind),
            Some(crate::radial::authoring::PendingRequestKind::StopNativePreview)
        );
    }

    fn seed_properties_search_result(
        driver: &mut RetainedDesignerDriver,
        target: &StableSelection,
        candidate: ActionBinding,
        query: &str,
    ) {
        let session = driver.editor.session.as_ref().expect("authoring session");
        let assigned = driver
            .editor
            .properties_draft
            .as_ref()
            .expect("Properties draft")
            .action_binding
            .clone();
        let scope = BindingEditorScope {
            surface: BindingEditorSurface::Properties,
            editor_session: session.editor_session,
            draft_generation: session.generation,
            target: target.clone(),
            slot: BindingEditorSlot::CellPrimary,
        };
        driver
            .editor
            .properties_draft
            .as_mut()
            .expect("Properties draft")
            .action_editor
            .seed_search_result_for_test(scope, assigned.as_ref(), query, candidate);
    }

    fn click_retained_named_fragment(
        driver: &mut RetainedDesignerDriver,
        output: &egui::FullOutput,
        fragment: &str,
        role: egui::accesskit::Role,
    ) -> egui::FullOutput {
        let point = accesskit_named_bounds_containing(output, fragment, role).center();
        driver
            .frame(vec![egui::Event::PointerMoved(point)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(point, true)])
            .drop_without_applying_deltas();
        driver.frame(vec![pointer_button_event(point, false)])
    }

    #[test]
    fn invalidated_native_update_syncs_terminal_stop_before_recovery_update() {
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.client = Some(client);

        editor.send_native_preview(false);
        let start = endpoint.request_rx.try_recv().expect("start request");
        let start_lease = crate::radial::authoring::NativePreviewLease {
            editor_session: start.editor_session(),
            generation: start.generation(),
            request_id: start.id(),
        };
        assert!(matches!(
            &start,
            crate::radial::authoring::AuthoringRequest::StartNativePreview { .. }
        ));
        assert!(editor.session.as_mut().unwrap().accept_reply(
            crate::radial::authoring::AuthoringReply::NativePreviewStarted {
                id: start.id(),
                generation: start.generation(),
                editor_session: start.editor_session(),
                lease: start_lease,
                sampled_context: crate::radial::context::InvocationContext::empty(1),
                diagnostics: Vec::new(),
            }
        ));

        editor.send_native_preview(true);
        let update = endpoint.request_rx.try_recv().expect("update request");
        assert!(matches!(
            &update,
            crate::radial::authoring::AuthoringRequest::UpdateNativePreview { .. }
        ));
        let menu_id = editor
            .session
            .as_ref()
            .expect("session")
            .draft
            .default_menu_id
            .clone();
        editor
            .session
            .as_mut()
            .expect("session")
            .mutate(
                crate::radial::authoring::DocumentMutation::RenameMenu {
                    id: menu_id,
                    name: "Edited while update is in flight".into(),
                },
                None,
                crate::radial::authoring::EditPhase::Atomic,
            )
            .unwrap();
        let session = editor.session.as_ref().unwrap();
        assert!(session.pending_native_preview.is_none());
        assert!(session.native_preview_lease.is_none());
        assert!(session.native_preview_may_be_open);

        editor.sync_native_preview_generation();
        let queued_stop = endpoint.request_rx.try_recv().expect("stop request");
        assert!(matches!(
            queued_stop,
            crate::radial::authoring::AuthoringRequest::StopNativePreview { .. }
        ));
        assert!(matches!(
            endpoint.request_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        assert_eq!(
            editor
                .session
                .as_ref()
                .and_then(|session| session.pending_native_preview)
                .map(|pending| pending.kind),
            Some(crate::radial::authoring::PendingRequestKind::StopNativePreview)
        );
    }

    #[test]
    fn discard_close_terminalizes_disconnected_preview_stop_without_retrying() {
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.client = Some(client);
        editor.close_intent = CloseIntent::DiscardRequested;
        editor
            .session
            .as_mut()
            .expect("session")
            .native_preview_may_be_open = true;
        drop(endpoint);

        editor.stop_native_preview();
        assert!(editor.close_stop_attempted);
        assert!(
            editor
                .session
                .as_ref()
                .is_some_and(|session| !session.native_preview_may_be_open)
        );
        editor.maybe_finish_close();
        assert!(!editor.open);
        assert!(editor.session.is_none());
        let close_requests = editor.close_stop_attempted;
        editor.maybe_finish_close();
        assert_eq!(editor.close_stop_attempted, close_requests);
    }

    #[test]
    fn queued_test_intents_wake_the_root_owner_without_draining_in_the_viewport() {
        let bridge = std::sync::Arc::new(DesignerIntentBridge::default());
        let wake_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let callback_count = std::sync::Arc::clone(&wake_count);
        bridge.set_wake(Some(std::sync::Arc::new(move || {
            callback_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })));
        let identity = AuthoringBindingEditorIdentity {
            scope: BindingEditorScope {
                surface: BindingEditorSurface::Inspector,
                editor_session: crate::radial::authoring::AuthoringSessionId(1),
                draft_generation: crate::radial::authoring::DraftGeneration(1),
                target: StableSelection::Cell {
                    menu_id: MenuId::new("main"),
                    ring_id: RingId::new("root"),
                    cell_id: CellId::new("cell-a"),
                },
                slot: BindingEditorSlot::CellPrimary,
            },
            assigned_binding_digest: 41,
            editor_epoch: 1,
            edit_generation: 1,
            query_generation: 1,
            query_request_generation: 1,
            search_request_generation: 0,
            test_request_generation: 1,
        };
        bridge.push(DesignerUiIntent::TestAction {
            identity,
            payload: Box::new(crate::gui::AuthoringProviderTestPayload {
                binding: crate::radial::model::ActionBinding::Contextual {
                    selector: crate::radial::model::TargetSelector::CapturedForeground,
                    action_id: crate::universal_actions::ActionId::new("test"),
                },
                invocation: InvocationContext::empty(0),
                history_query: String::new(),
            }),
        });
        assert_eq!(wake_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(bridge.drain().len(), 1);
    }

    #[test]
    fn file_dialog_enqueue_and_return_wake_the_correct_owners() {
        let bridge = std::sync::Arc::new(DesignerIntentBridge::default());
        let root_wakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let viewport_wakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let root_count = std::sync::Arc::clone(&root_wakes);
        let root_bridge = std::sync::Arc::clone(&bridge);
        bridge.set_wake(Some(std::sync::Arc::new(move || {
            // The callback can inspect the queue: enqueue has released the
            // queue lock before waking ROOT.
            assert!(root_bridge.has_pending_file_dialog());
            root_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })));
        let viewport_count = std::sync::Arc::clone(&viewport_wakes);
        bridge.set_viewport_wake(Some(std::sync::Arc::new(move || {
            viewport_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })));
        bridge.enqueue_file_dialog(DesignerFileDialogRequest::PreviewPackage);
        bridge.enqueue_file_dialog(DesignerFileDialogRequest::ReplaceBackup);
        assert_eq!(root_wakes.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(bridge.take_file_dialog().is_some());
        bridge.finish_file_dialog();
        assert_eq!(viewport_wakes.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            root_wakes.load(std::sync::atomic::Ordering::SeqCst),
            3,
            "a queued dialog must schedule another root-owned drain"
        );
        assert!(bridge.take_file_dialog().is_some());
        bridge.finish_file_dialog();
        assert_eq!(viewport_wakes.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(root_wakes.load(std::sync::atomic::Ordering::SeqCst), 3);
        bridge.clear();
        assert!(!bridge.has_pending_file_dialog());
    }

    #[test]
    fn debounced_preferences_wake_root_and_close_flush_survives_disposal() {
        let bridge = std::sync::Arc::new(DesignerIntentBridge::default());
        let root_wakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let root_count = std::sync::Arc::clone(&root_wakes);
        let root_bridge = std::sync::Arc::clone(&bridge);
        bridge.set_wake(Some(std::sync::Arc::new(move || {
            assert!(root_bridge.has_pending_preferences());
            root_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })));

        let mut editor = RadialEditorState::default();
        editor.intent_bridge = std::sync::Arc::clone(&bridge);
        editor.preferences_dirty = true;
        let now = Instant::now();
        editor
            .preference_debounce
            .mark_changed(now - Duration::from_millis(300));
        editor.enqueue_preferences_ready_if_due();
        assert_eq!(root_wakes.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(editor.take_preferences_for_persist().is_some());
        assert!(!bridge.has_pending_preferences());

        // The close path queues the final flush before the editor disposes
        // its callbacks.  ROOT can still consume the snapshot afterward.
        editor.open_test_snapshot();
        editor.preferences_dirty = true;
        editor
            .preference_debounce
            .mark_changed(now - Duration::from_millis(300));
        editor.request_close();
        assert_eq!(root_wakes.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(editor.session.is_none());
        assert!(bridge.has_pending_preferences());
        assert!(editor.take_preferences_for_persist().is_some());
        assert!(!bridge.has_pending_preferences());
    }

    #[test]
    fn properties_draft_is_stable_across_frames_and_rejects_stale_apply() {
        let document = RadialDocument::starter();
        let cell = &document.menus[0].rings[0].cells[0];
        let original_label = cell.label.clone();
        let target = StableSelection::Cell {
            menu_id: document.menus[0].id.clone(),
            ring_id: document.menus[0].rings[0].id.clone(),
            cell_id: cell.id.clone(),
        };
        let generation = crate::radial::authoring::DraftGeneration(7);
        let mut draft = PropertiesDraft::from_cell(target.clone(), generation, cell);
        draft.label = "edited over several frames".into();
        assert!(draft.is_current(&target, generation));
        assert!(!draft.is_current(&target, crate::radial::authoring::DraftGeneration(8)));
        assert_eq!(draft.label, "edited over several frames");
        // Cancel is represented by dropping the draft; no document is ever
        // touched by local popup edits.
        drop(draft);
        assert_eq!(
            PropertiesDraft::from_cell(target, generation, cell).label,
            original_label
        );
    }

    #[test]
    fn dirty_cell_popup_survives_native_close_until_explicit_resolution() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_ref().unwrap();
        let cell = &session.draft.menus[0].rings[0].cells[0];
        let target = StableSelection::Cell {
            menu_id: session.draft.menus[0].id.clone(),
            ring_id: session.draft.menus[0].rings[0].id.clone(),
            cell_id: cell.id.clone(),
        };
        let mut draft = PropertiesDraft::from_cell(target.clone(), session.generation, cell);
        draft.label.push_str(" edited");
        editor.properties_popup = Some(target);
        editor.properties_draft = Some(draft);

        editor.request_close();

        assert!(editor.open);
        assert!(editor.close_prompt);
        assert!(editor.properties_popup_dirty());
        assert!(editor.properties_draft.is_some());
    }

    #[test]
    fn repeated_same_geometry_proposals_receive_distinct_preview_identities() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_ref().unwrap();
        let menu_id = session.draft.menus[0].id.clone();
        let ring_id = session.draft.menus[0].rings[0].id.clone();
        let requested_len = session.draft.menus[0].rings[0].cells.len() + 1;
        let generation = session.generation;
        let first = menu::propose_ring_resize(
            &session.draft,
            generation,
            &menu_id,
            &ring_id,
            requested_len,
            vec![CellId::new("proposal-cell-a")],
        )
        .unwrap();
        let second = menu::propose_ring_resize(
            &session.draft,
            generation,
            &menu_id,
            &ring_id,
            requested_len,
            vec![CellId::new("proposal-cell-b")],
        )
        .unwrap();
        assert_eq!(first.requested_len, second.requested_len);
        assert_eq!(first.proposed_radius, second.proposed_radius);
        let first_new_id = first.document.menus[0].rings[0]
            .cells
            .last()
            .unwrap()
            .id
            .clone();
        let second_new_id = second.document.menus[0].rings[0]
            .cells
            .last()
            .unwrap()
            .id
            .clone();
        assert_eq!(first_new_id, CellId::new("proposal-cell-a"));
        assert_eq!(second_new_id, CellId::new("proposal-cell-b"));
        assert_ne!(first_new_id, second_new_id);

        editor.install_ring_proposal(first);
        let first_nonce = editor.ring_proposal_nonce.unwrap();
        let first_token = ring_proposal_token(editor.ring_proposal.as_ref().unwrap(), first_nonce);
        editor.install_ring_proposal(second);
        let second_nonce = editor.ring_proposal_nonce.unwrap();
        let second_token =
            ring_proposal_token(editor.ring_proposal.as_ref().unwrap(), second_nonce);
        assert_ne!(first_token, second_token);
        assert!(
            !editor
                .preview
                .candidate_is_prepared(editor.session.as_ref().unwrap(), &second_token)
        );
    }

    #[test]
    fn diagnostic_display_model_bounds_actionables_and_discloses_overflow() {
        let actionables = (0..=crate::radial::diagnostics::MAX_RADIAL_DIAGNOSTICS).map(|index| {
            crate::radial::diagnostics::RadialDiagnostic::new(
                crate::radial::diagnostics::RadialDiagnosticSeverity::Error,
                crate::radial::diagnostics::RadialDiagnosticKind::AssetUnavailable(
                    crate::radial::assets::AssetDiagnostic::NotFound,
                ),
                crate::radial::diagnostics::RadialDiagnosticSource::Asset {
                    menu_id: crate::radial::model::MenuId::new("menu"),
                    identity: format!("missing-{index}"),
                },
                index,
                "asset missing",
            )
        });
        let expected = crate::radial::diagnostics::RadialDiagnostic::from_font(
            &crate::radial::model::MenuId::new("menu"),
            &crate::radial::model::CellId::new("cell"),
            "long label",
            12_000_u32,
            &crate::radial::font_cache::FontDiagnostic::LabelTruncated,
        );
        let model = radial_diagnostic_display_model(actionables.chain([expected]));
        assert_eq!(
            model.actionable.len(),
            crate::radial::diagnostics::MAX_RADIAL_DIAGNOSTICS - 1
        );
        assert!(model.expected_layout.is_empty());
        assert_eq!(
            model.omitted,
            Some(crate::radial::diagnostics::RadialDiagnosticOmission {
                total: 3,
                actionable: 2,
                expected_layout: 1,
            })
        );
    }

    #[test]
    fn post_render_move_uses_stable_ids_and_selection_survives_reorder() {
        let mut document = RadialDocument::starter();
        document.menus[0].rings[0].cells[0].content = CellContent::Spacer;
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut editor = RadialEditorState::default();
        editor.open = true;
        editor.session = Some(RadialAuthoringSession::new(snapshot));
        let session = editor.session.as_mut().unwrap();
        let menu_id = session.draft.menus[0].id.clone();
        let ring_id = session.draft.menus[0].rings[0].id.clone();
        let cell_id = session.draft.menus[0].rings[0].cells[1].id.clone();
        session.select(Some(StableSelection::Cell {
            menu_id: menu_id.clone(),
            ring_id: ring_id.clone(),
            cell_id: cell_id.clone(),
        }));
        let expected_generation = session.generation;
        editor.post_render.push(EditorCommand::MoveCell {
            source_menu: menu_id.clone(),
            source_ring: ring_id.clone(),
            cell: cell_id.clone(),
            destination_menu: menu_id.clone(),
            destination_ring: ring_id.clone(),
            index: 0,
            expected_generation,
        });
        editor.apply_post_render();
        assert_eq!(
            editor.session.as_ref().unwrap().draft.menus[0].rings[0].cells[0].id,
            cell_id
        );
        assert!(
            matches!(editor.session.as_ref().unwrap().selection.as_ref(), Some(StableSelection::Cell { cell_id: selected, .. }) if selected == &cell_id)
        );
        assert!(
            matches!(editor.focus_restore.as_ref(), Some(StableSelection::Cell { cell_id: focused, .. }) if focused == &cell_id)
        );
    }

    #[test]
    fn preview_zoom_and_presets_never_dirty_the_authoring_session() {
        let document = RadialDocument::starter();
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut editor = RadialEditorState::default();
        editor.session = Some(RadialAuthoringSession::new(snapshot));
        let before = editor.session.as_ref().unwrap().draft.clone();
        editor.preview_zoom = 1.75;
        editor.preview_preset = PreviewPreset::HighDpi;
        assert_eq!(editor.session.as_ref().unwrap().draft, before);
        assert!(!editor.session.as_ref().unwrap().is_dirty());
    }

    #[test]
    fn widget_dispatcher_coalesces_one_drag_without_erasing_older_history() {
        let document = RadialDocument::starter();
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut session = RadialAuthoringSession::new(snapshot);
        let menu_id = session.draft.menus[0].id.clone();
        menu::rename_menu(
            &mut session,
            menu_id.clone(),
            "Older edit".into(),
            EditPhase::Atomic,
        )
        .unwrap();
        for (radius, signals) in [
            (
                100.0,
                WidgetEditSignals {
                    drag_started: true,
                    ended: false,
                },
            ),
            (120.0, WidgetEditSignals::default()),
            (
                140.0,
                WidgetEditSignals {
                    drag_started: false,
                    ended: true,
                },
            ),
        ] {
            let mut edited = (*session.draft).clone();
            edited.menus[0].rings[0].radius = radius;
            dispatch_widget_document_edit(
                &mut session,
                edited,
                "ring:starter/ring-0".into(),
                "radius",
                signals,
            )
            .unwrap();
        }
        assert_eq!(session.draft.menus[0].rings[0].radius, 140.0);
        assert!(session.undo());
        assert_ne!(session.draft.menus[0].rings[0].radius, 140.0);
        assert_eq!(session.draft.menus[0].name, "Older edit");
        assert!(session.undo());
        assert_ne!(session.draft.menus[0].name, "Older edit");
    }

    #[test]
    fn continuous_editor_categories_use_stable_field_dispatch_and_previewed_resize() {
        let source = include_str!("mod.rs");
        let production = production_source(source);
        for field in [
            "hover_dwell_ms",
            "center_radius",
            "dynamic.max_items",
            "dynamic.query",
            "tooltip",
            "icon.resource_index",
            "priority",
            "window_title_contains",
            "chord",
            "image_directories[",
            "sound_directories[",
            "resource.index",
        ] {
            assert!(
                production.contains(field),
                "missing continuous field {field}"
            );
        }
        assert!(production.contains("continuous_widget_edit"));
        assert!(production.contains("dispatch_widget_document_edit"));
        assert!(production.contains("ring_resize_drafts"));
        assert!(production.contains("Preview proposal"));
        assert!(production.contains("install_ring_proposal"));
        assert!(production.contains("menu::apply_ring_proposal"));
        assert!(production.contains("ring_proposal_nonce"));
        assert!(production.contains("candidate_is_prepared"));
        assert!(
            !production.contains("menu::apply_resize"),
            "GUI slot-count paths must not bypass preview and explicit Apply"
        );
    }

    #[test]
    fn gui_uses_authoring_requests_without_store_ownership_or_local_packaging() {
        let source = include_str!("mod.rs");
        let production = production_source(source);
        assert!(!production.contains("RadialStore"));
        assert!(!production.contains("NativeHost"));
        assert!(!production.contains("RADIAL_ASSETS_DIRECTORY"));
        assert!(!production.contains("import_export::export_bytes"));
        assert!(production.contains("request_export_package"));
        assert!(production.contains("request_replace_package"));
        assert!(production.contains("request_start_native_preview"));
        assert!(production.contains("request_update_native_preview"));
        assert!(production.contains("request_stop_native_preview"));
    }

    #[test]
    fn populated_ring_delete_relocates_cells_atomically_with_stable_ids() {
        let mut document = RadialDocument::starter();
        let menu_id = document.menus[0].id.clone();
        let source = document.menus[0].rings[0].id.clone();
        let moved: Vec<_> = document.menus[0].rings[0]
            .cells
            .iter()
            .map(|cell| cell.id.clone())
            .collect();
        let mut second = document.menus[0].rings[0].clone();
        second.id = RingId::new("destination");
        second.cells.clear();
        document.menus[0].rings.push(second);
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut session = RadialAuthoringSession::new(snapshot);
        relocate_and_delete_ring(&mut session, &menu_id, &source, &RingId::new("destination"))
            .unwrap();
        assert_eq!(session.draft.menus[0].rings.len(), 1);
        assert_eq!(
            session.draft.menus[0].rings[0]
                .cells
                .iter()
                .map(|cell| cell.id.clone())
                .collect::<Vec<_>>(),
            moved
        );
    }

    #[test]
    fn assignment_helpers_cover_cell_secondary_and_all_menu_pointer_slots() {
        let document = RadialDocument::starter();
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut session = RadialAuthoringSession::new(snapshot);
        let menu_id = session.draft.menus[0].id.clone();
        let ring_id = session.draft.menus[0].rings[0].id.clone();
        let cell_id = session.draft.menus[0].rings[0].cells[0].id.clone();
        let binding = crate::radial::model::ActionBinding::Persisted {
            action: crate::universal_actions::PersistedUniversalActionRef {
                target: None,
                action_id: crate::universal_actions::ActionId::new("test"),
            },
        };
        assign_alternate_click(
            &mut session,
            &menu_id,
            &ring_id,
            &cell_id,
            crate::radial::model::ClickGesture::Secondary,
            binding.clone(),
        )
        .expect("alternate action assignment succeeds");
        for slot in [
            MenuActionSlot::CenterPrimary,
            MenuActionSlot::CenterSecondary,
            MenuActionSlot::BackgroundPrimary,
            MenuActionSlot::BackgroundSecondary,
        ] {
            assign_menu_action(&mut session, &menu_id, slot, binding.clone())
                .expect("menu action assignment succeeds");
        }
        let menu = &session.draft.menus[0];
        assert!(menu.center_action.is_some() && menu.center_secondary_action.is_some());
        assert!(menu.background_action.is_some() && menu.background_secondary_action.is_some());
        assert_eq!(menu.rings[0].cells[0].alternate_clicks.len(), 1);
    }

    #[test]
    fn alternate_action_assignment_rejection_preserves_authoring_state_and_surfaces_error() {
        use crate::radial::authoring::{AuthoringRequest, authoring_control_service};

        let (client, endpoint) = authoring_control_service();
        let snapshot = AuthoringSnapshot::new(
            std::sync::Arc::new(RadialDocument::starter()),
            "pending-alternate-assignment-test",
        );
        let mut session = RadialAuthoringSession::new(snapshot);
        let menu_id = session.draft.menus[0].id.clone();
        let ring_id = session.draft.menus[0].rings[0].id.clone();
        let cell_id = session.draft.menus[0].rings[0].cells[0].id.clone();
        let selection = StableSelection::Cell {
            menu_id: menu_id.clone(),
            ring_id: ring_id.clone(),
            cell_id: cell_id.clone(),
        };
        session.select(Some(selection.clone()));
        session.require_authoritative_snapshot();
        let request = session.request_snapshot().expect("snapshot request");
        client.send(request).expect("snapshot request is queued");
        assert!(matches!(
            endpoint.request_rx.try_recv(),
            Ok(AuthoringRequest::Snapshot { .. })
        ));
        let pending = session.pending_request.expect("request remains pending");

        let before_document = std::sync::Arc::clone(&session.draft);
        let before_generation = session.generation;
        let before_history = session.acceptance_history_depths();
        let binding = crate::radial::model::ActionBinding::Persisted {
            action: crate::universal_actions::PersistedUniversalActionRef {
                target: None,
                action_id: crate::universal_actions::ActionId::new("pending-test"),
            },
        };
        let result = assign_alternate_click(
            &mut session,
            &menu_id,
            &ring_id,
            &cell_id,
            crate::radial::model::ClickGesture::Secondary,
            binding,
        );
        assert_eq!(result, Err(AuthoringError::RequestPending));
        report_action_assignment_result(&mut session, result);

        assert!(std::sync::Arc::ptr_eq(&session.draft, &before_document));
        assert_eq!(session.generation, before_generation);
        assert_eq!(session.acceptance_history_depths(), before_history);
        assert_eq!(session.selection.as_ref(), Some(&selection));
        assert_eq!(session.pending_request, Some(pending));
        assert_eq!(
            session.last_error.as_deref(),
            Some("Action assignment failed: RequestPending")
        );

        let mut editor = RadialEditorState::default();
        editor.open = true;
        editor.client = Some(client);
        editor.session = Some(session);
        let mut driver = RetainedDesignerDriver::new(editor);
        let output = driver.frame(Vec::new());
        assert!(
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .expect("Designer exposes its feedback")
                .nodes
                .iter()
                .any(|(_, node)| node.role() == egui::accesskit::Role::Label
                    && node.value().is_some_and(
                        |name| name.contains("Error: Action assignment failed: RequestPending")
                    ))
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn accessibility_contract_names_blank_controls_and_restores_stable_focus() {
        let source = include_str!("mod.rs");
        let production = production_source(source);
        let action_editor_production = include_str!("action_editor.rs");
        let compact_production = production.split_whitespace().collect::<String>();
        assert!(compact_production.contains("WidgetInfo::selected(egui::WidgetType::Checkbox"));
        assert!(production.contains("egui::WidgetType::ColorButton"));
        assert!(production.contains("egui::WidgetType::ComboBox"));
        assert!(production.contains("info.label = Some(accessible_name.into())"));
        assert!(production.contains("focus_restore.as_ref() == Some(&ring_selection)"));
        assert!(production.contains(".and_then(StableSelection::primary_cell)"));
        assert!(production.contains("response.request_focus()"));
        assert!(production.contains("self.focus_restore = session.selection.clone()"));
        assert!(production.contains("ui.visuals().error_fg_color"));
        assert!(production.contains("ui.visuals().warn_fg_color"));
        assert!(!production.contains("Color32::RED"));
        assert!(!production.contains("Color32::YELLOW"));

        // These visible words are intentional redundant state channels: unavailable,
        // error, selected, and destructive operations are not communicated by color
        // or icon alone.
        for semantic_text in [
            "Delete blocked:",
            "Loading the authoritative radial configuration",
            "Remove alternate action",
            "Move inward",
            "Move outward",
        ] {
            assert!(
                production.contains(semantic_text),
                "missing {semantic_text}"
            );
        }
        assert!(action_editor_production.contains("Availability:"));
    }

    #[test]
    fn headless_accesskit_tree_exposes_style_control_roles_names_and_state() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut scratch = String::new();
                let inherited_color = skin_editor::with_payload(serde_json::json!({
                    "red": 12,
                    "green": 34,
                    "blue": 56,
                    "alpha": 78
                }));
                let inherited_toggle = skin_editor::with_payload(serde_json::json!(false));
                let _ = style_value_widget(
                    ui,
                    skin_editor::StyleControlKind::Color,
                    &serde_json::Value::Null,
                    &inherited_color,
                    &mut scratch,
                    &[],
                    "text.color",
                    None,
                    None,
                );
                let _ = style_value_widget(
                    ui,
                    skin_editor::StyleControlKind::Toggle,
                    &serde_json::Value::Null,
                    &inherited_toggle,
                    &mut scratch,
                    &[],
                    "text.visible",
                    None,
                    None,
                );
            });
        });
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("AccessKit tree update");
        assert!(update.nodes.iter().any(|(_, node)| {
            node.role() == egui::accesskit::Role::ColorWell
                && node
                    .label()
                    .is_some_and(|name| name.contains("text.color: rgba(12, 34, 56, 78)"))
        }));
        assert!(update.nodes.iter().any(|(_, node)| {
            node.role() == egui::accesskit::Role::CheckBox
                && node.label() == Some("text.visible")
                && node.toggled() == Some(egui::accesskit::Toggled::False)
        }));
        output.drop_without_applying_deltas();
    }

    #[test]
    fn retained_headless_viewport_accepts_semantic_click_after_snapshot_and_tabs() {
        use crate::radial::authoring::{
            AuthoringReply, AuthoringSnapshot, RadialAuthoringSession, authoring_control_service,
        };

        let (client, endpoint) = authoring_control_service();
        let placeholder = std::sync::Arc::new(RadialDocument::starter());
        let mut session = RadialAuthoringSession::new(AuthoringSnapshot::new(
            std::sync::Arc::clone(&placeholder),
            "headless-placeholder",
        ));
        session.require_authoritative_snapshot();
        let request = session
            .request_snapshot()
            .expect("initial snapshot request");
        let (request_id, generation, editor_session) =
            (request.id(), request.generation(), request.editor_session());
        client.send(request).expect("enqueue snapshot request");
        assert!(matches!(
            endpoint.request_rx.try_recv(),
            Ok(crate::radial::authoring::AuthoringRequest::Snapshot { .. })
        ));

        let mut editor = RadialEditorState::default();
        editor.open = true;
        editor.client = Some(client);
        editor.session = Some(session);
        let mut driver = RetainedDesignerDriver::new(editor);

        let initial = driver.frame(Vec::new());
        assert!(
            driver
                .editor
                .session
                .as_ref()
                .is_some_and(RadialAuthoringSession::is_initial_snapshot_pending)
        );
        assert!(
            initial
                .platform_output
                .accesskit_update
                .as_ref()
                .expect("initial AccessKit tree")
                .nodes
                .iter()
                .any(|(_, node)| node.role() == egui::accesskit::Role::Label
                    && node.value().is_some_and(
                        |name| name.contains("Loading the authoritative radial configuration")
                    ))
        );
        let pending_click =
            accesskit_named_bounds(&initial, "New Menu", egui::accesskit::Role::Button).center();
        initial.drop_without_applying_deltas();
        let (pending_menu_count, pending_generation) = {
            let session = driver.editor.session.as_ref().expect("authoring session");
            (session.draft.menus.len(), session.generation)
        };
        driver
            .frame(vec![egui::Event::PointerMoved(pending_click)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(pending_click, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(pending_click, false)])
            .drop_without_applying_deltas();
        let session = driver.editor.session.as_ref().expect("authoring session");
        assert!(session.is_initial_snapshot_pending());
        assert_eq!(session.draft.menus.len(), pending_menu_count);
        assert_eq!(session.generation, pending_generation);
        assert!(!session.is_dirty());

        let mut authoritative = RadialDocument::starter();
        authoritative.menus[0].name = "Authoritative fixture".to_string();
        endpoint
            .reply_tx
            .send(AuthoringReply::Snapshot {
                id: request_id,
                generation,
                editor_session,
                snapshot: AuthoringSnapshot::new(
                    std::sync::Arc::new(authoritative),
                    "headless-authoritative",
                ),
            })
            .expect("enqueue matching snapshot reply");

        let enabled = driver.frame(Vec::new());
        let session = driver.editor.session.as_ref().expect("authoring session");
        assert!(!session.is_initial_snapshot_pending());
        assert_eq!(session.draft.menus[0].name, "Authoritative fixture");
        let mut menus_before = session.draft.menus.len();
        let mut generation_before = session.generation;
        let add_menu = accesskit_named_bounds(&enabled, "New Menu", egui::accesskit::Role::Button);
        let click_at = add_menu.center();
        enabled.drop_without_applying_deltas();

        driver
            .frame(vec![egui::Event::PointerMoved(click_at)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(click_at, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(click_at, false)])
            .drop_without_applying_deltas();
        let session = driver.editor.session.as_ref().expect("authoring session");
        assert_eq!(session.draft.menus.len(), menus_before + 1);
        assert!(session.is_dirty());
        assert!(session.generation > generation_before);
        menus_before = session.draft.menus.len();
        generation_before = session.generation;

        let after_click = driver.frame(Vec::new());
        assert!(
            after_click
                .platform_output
                .accesskit_update
                .as_ref()
                .is_some_and(|update| !update.nodes.is_empty())
        );
        assert_eq!(
            driver.editor.session.as_ref().unwrap().draft.menus.len(),
            menus_before
        );
        assert_eq!(
            driver.editor.session.as_ref().unwrap().generation,
            generation_before
        );

        assert!(!driver.editor.tree_visible);
        let tree_button =
            accesskit_named_bounds(&after_click, "Tree", egui::accesskit::Role::Button);
        let tree_click = tree_button.center();
        after_click.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(tree_click)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(tree_click, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(tree_click, false)])
            .drop_without_applying_deltas();
        assert!(driver.editor.tree_visible);

        let after_tree_click = driver.frame(Vec::new());
        let inspector_button = accesskit_named_bounds(
            &after_tree_click,
            "Inspector",
            egui::accesskit::Role::Button,
        );
        let inspector_click = inspector_button.center();
        after_tree_click.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(inspector_click)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(inspector_click, true)])
            .drop_without_applying_deltas();
        let inspector_toggled = driver.frame(vec![pointer_button_event(inspector_click, false)]);
        assert!(driver.editor.inspector_visible);
        let menu_name = accesskit_value_bounds(
            &inspector_toggled,
            egui::accesskit::Role::TextInput,
            "New menu",
        );
        let menu_name_click = menu_name.center();
        inspector_toggled.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(menu_name_click)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(menu_name_click, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(menu_name_click, false)])
            .drop_without_applying_deltas();

        let before_tab = driver.context.memory(|memory| memory.focused());
        assert!(
            before_tab.is_some(),
            "clicking the semantic menu-name TextInput gives it egui focus"
        );
        driver
            .frame(vec![key_event(
                egui::Key::Tab,
                egui::Modifiers::default(),
                true,
            )])
            .drop_without_applying_deltas();
        let after_tab_press = driver.context.memory(|memory| memory.focused());
        assert!(
            after_tab_press.is_some(),
            "Tab keeps an eligible control focused"
        );
        assert_ne!(after_tab_press, before_tab, "Tab moves egui keyboard focus");
        driver
            .frame(vec![key_event(
                egui::Key::Tab,
                egui::Modifiers::default(),
                false,
            )])
            .drop_without_applying_deltas();
    }

    #[test]
    fn jump_mode_radial_tree_selection_only_selects_and_focuses_the_authored_ring() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.tree_visible = true;
        editor.session.as_mut().unwrap().font_catalog_loaded = true;
        let menu_id = editor
            .session
            .as_ref()
            .expect("authoring session")
            .draft
            .menus[0]
            .id
            .clone();
        editor
            .preferences
            .expanded_sections
            .insert(format!("menu:{menu_id}"), true);
        let original =
            std::sync::Arc::clone(&editor.session.as_ref().expect("authoring session").draft);
        let mut driver = RetainedDesignerDriver::new(editor);
        driver.frame.jump_mode_enabled = true;

        driver.frame(Vec::new()).drop_without_applying_deltas();
        let initial_owner = NavigationOwner::Dialog {
            kind: NavigationDialogKind::RadialDesigner,
            instance: 1,
        };
        assert_eq!(
            driver.editor.native_jump_mode.frame_navigation_owner(),
            NavigationOwner::Blocked,
            "the first retained frame establishes the child viewport owner"
        );
        driver.frame(Vec::new()).drop_without_applying_deltas();
        assert_eq!(
            driver.editor.native_jump_mode.frame_navigation_owner(),
            initial_owner,
            "the following completed frame collects under the exact child owner"
        );
        let ring_target = driver
            .editor
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .find(|target| {
                target.metadata().label() == "Ring"
                    && target.metadata().kind() == crate::gui::jump_mode::TargetKind::Action
            })
            .cloned()
            .expect("visible authored ring has a Select target");
        assert!(ring_target.eligible());
        assert!(ring_target.clipped_rect().is_positive());

        let leader = egui::Modifiers::SHIFT;
        driver
            .frame_with_modifiers(
                vec![
                    key_event(egui::Key::Space, leader, true),
                    egui::Event::Text(" ".into()),
                ],
                leader,
            )
            .drop_without_applying_deltas();
        let hint = driver
            .editor
            .native_jump_mode
            .active_session()
            .expect("leader starts the viewport hint session")
            .hint_map()
            .hint_for(ring_target.id())
            .expect("the visible ring is in the captured hint map")
            .as_str()
            .to_owned();
        driver
            .frame_with_modifiers(vec![key_event(egui::Key::Space, leader, false)], leader)
            .drop_without_applying_deltas();
        let mut focus_after_selection = None;
        driver
            .frame_with_observer(
                vec![egui::Event::Text(hint)],
                egui::Modifiers::default(),
                |ctx, _| {
                    assert_eq!(
                        ctx.viewport_id(),
                        radial_designer_viewport_id(),
                        "focus is inspected while the retained child viewport is current"
                    );
                    focus_after_selection = ctx.memory(|memory| memory.focused());
                },
            )
            .drop_without_applying_deltas();

        let session = driver.editor.session.as_ref().expect("authoring session");
        let ring = &session.draft.menus[0].rings[0];
        assert_eq!(
            session.selection,
            Some(StableSelection::Ring {
                menu_id: menu_id.clone(),
                ring_id: ring.id.clone(),
            }),
            "hint completion selects only the authored ring"
        );
        assert!(
            std::sync::Arc::ptr_eq(&session.draft, &original),
            "navigation does not replace or publish the document draft"
        );
        assert_eq!(session.pending_request, None);
        assert!(!session.is_dirty());
        assert_eq!(
            focus_after_selection,
            Some(ring_target.response_id()),
            "Select completion focuses the real ring response in its viewport"
        );
        assert!(
            driver.editor.native_jump_mode.active_session().is_none(),
            "the completed hint session is retired"
        );
    }

    #[test]
    fn jump_mode_radial_tree_combo_choice_focus_does_not_select_the_option() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let original_selection = {
            let session = editor.session.as_mut().unwrap();
            let menu_id = session.draft.default_menu_id.clone();
            let first_ring_id = {
                let document = std::sync::Arc::make_mut(&mut session.draft);
                let menu = document
                    .menus
                    .iter_mut()
                    .find(|menu| menu.id == menu_id)
                    .unwrap();
                let first_ring_id = menu.rings[0].id.clone();
                menu.rings.push(crate::radial::model::RingDefinition {
                    id: crate::radial::model::RingId::new("jump-mode-test-second-ring"),
                    radius: 120.0,
                    cell_radius: 24.0,
                    rotation_degrees: 0.0,
                    gap: 0.0,
                    cells: Vec::new(),
                    style: crate::radial::model::RingStyleLayer::default(),
                });
                first_ring_id
            };
            let selection = StableSelection::Ring {
                menu_id,
                ring_id: first_ring_id,
            };
            session.select(Some(selection.clone()));
            selection
        };
        let original = std::sync::Arc::clone(&editor.session.as_ref().unwrap().draft);
        let original_dirty = editor.session.as_ref().unwrap().is_dirty();
        let mut driver = RetainedDesignerDriver::new(editor);
        driver.frame.jump_mode_enabled = true;

        driver.frame(Vec::new()).drop_without_applying_deltas();
        driver.frame(Vec::new()).drop_without_applying_deltas();
        let combo = driver
            .editor
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .find(|target| {
                target.metadata().label() == "Ring selector"
                    && target.metadata().kind() == crate::gui::jump_mode::TargetKind::ComboBox
            })
            .cloned()
            .expect("the visible Ring combo is registered from its real response");
        let combo_point = combo.clipped_rect().center();
        driver
            .frame(vec![egui::Event::PointerMoved(combo_point)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(combo_point, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(combo_point, false)])
            .drop_without_applying_deltas();
        let mut combo_open_in_child = false;
        driver
            .frame_with_observer(Vec::new(), egui::Modifiers::default(), |ctx, _| {
                combo_open_in_child = egui::ComboBox::is_open(ctx, combo.response_id());
            })
            .drop_without_applying_deltas();
        assert!(
            combo_open_in_child,
            "the child viewport retains the open combo"
        );

        let ring_options = driver
            .editor
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .filter(|target| {
                target.metadata().label() == "Ring option"
                    && target.metadata().kind() == crate::gui::jump_mode::TargetKind::Selectable
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(ring_options.len(), 2);
        let option = ring_options
            .into_iter()
            .nth(1)
            .expect("the second real Ring option is registered as focus-only");
        assert!(option.eligible());
        assert!(option.clipped_rect().is_positive());

        let leader = egui::Modifiers::SHIFT;
        driver
            .frame_with_modifiers(
                vec![
                    key_event(egui::Key::Space, leader, true),
                    egui::Event::Text(" ".into()),
                ],
                leader,
            )
            .drop_without_applying_deltas();
        let hint = driver
            .editor
            .native_jump_mode
            .active_session()
            .expect("the current popup is owned by the child viewport")
            .hint_map()
            .hint_for(option.id())
            .expect("the visible Ring option is in the hint map")
            .as_str()
            .to_owned();
        driver
            .frame_with_modifiers(vec![key_event(egui::Key::Space, leader, false)], leader)
            .drop_without_applying_deltas();
        let mut focused_after_option = None;
        driver
            .frame_with_observer(
                vec![egui::Event::Text(hint)],
                egui::Modifiers::default(),
                |ctx, _| focused_after_option = ctx.memory(|memory| memory.focused()),
            )
            .drop_without_applying_deltas();

        let session = driver.editor.session.as_ref().expect("authoring session");
        assert_eq!(session.selection, Some(original_selection));
        assert!(std::sync::Arc::ptr_eq(&session.draft, &original));
        assert_eq!(session.pending_request, None);
        assert_eq!(
            session.is_dirty(),
            original_dirty,
            "focus-only option completion preserves the setup document's dirty state"
        );
        assert_eq!(
            focused_after_option,
            Some(option.response_id()),
            "focus-only option completion focuses the actual popup response in its viewport"
        );
    }

    #[test]
    fn jump_mode_designer_ownership_radial_viewport_replacement_and_pending_picker_are_isolated() {
        use crate::gui::jump_mode::ui::{NavigationDialogKind, NavigationOwner};
        use crate::gui::jump_mode::{SurfaceId, TargetKind};

        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.tree_visible = true;
        let menu_id = editor
            .session
            .as_ref()
            .expect("authoring session")
            .draft
            .menus[0]
            .id
            .clone();
        editor
            .preferences
            .expanded_sections
            .insert(format!("menu:{menu_id}"), true);
        let original = std::sync::Arc::clone(&editor.session.as_ref().unwrap().draft);
        let mut driver = RetainedDesignerDriver::new(editor);
        driver.frame.jump_mode_enabled = true;
        let first_owner = NavigationOwner::Dialog {
            kind: NavigationDialogKind::RadialDesigner,
            instance: 1,
        };

        driver.frame(Vec::new()).drop_without_applying_deltas();
        driver.frame(Vec::new()).drop_without_applying_deltas();
        assert_eq!(
            driver.editor.native_jump_mode.frame_navigation_owner(),
            first_owner
        );
        let ring_target = driver
            .editor
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .find(|target| {
                target.metadata().label() == "Ring"
                    && target.metadata().kind() == TargetKind::Action
            })
            .cloned()
            .expect("the visible tree provides an actual authored Ring response");
        assert_eq!(ring_target.viewport(), radial_designer_viewport_id());
        assert!(ring_target.clipped_rect().is_positive());
        let original_selection = driver.editor.session.as_ref().unwrap().selection.clone();

        let root_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1_000.0, 720.0),
            )),
            viewport_id: egui::ViewportId::ROOT,
            focused: true,
            ..Default::default()
        };
        let context = driver.context.clone();
        let frame = &driver.frame;
        let editor = &mut driver.editor;
        context
            .run_ui(root_input, |ui| {
                editor.viewport_ui(ui, frame, egui::ViewportClass::Deferred);
            })
            .drop_without_applying_deltas();
        let session = driver.editor.session.as_ref().unwrap();
        assert!(std::sync::Arc::ptr_eq(&session.draft, &original));
        assert_eq!(session.selection, original_selection);
        assert_eq!(session.pending_request, None);

        // The pending native picker is tested only through the existing bridge:
        // no native dialog is processed or opened by this fixture.
        let bridge = driver.editor.intent_bridge();
        bridge.enqueue_file_dialog(DesignerFileDialogRequest::ExportPackage {
            roots: vec![menu_id.clone()],
        });
        assert!(bridge.has_pending_file_dialog());
        driver.frame(Vec::new()).drop_without_applying_deltas();
        assert_eq!(
            driver.editor.native_jump_mode.frame_navigation_owner(),
            NavigationOwner::Blocked
        );
        assert!(
            driver
                .editor
                .native_jump_mode
                .retained_registry()
                .targets()
                .is_empty()
        );
        let queued = bridge
            .take_file_dialog()
            .expect("the fixture queued a request");
        assert!(matches!(
            queued,
            DesignerFileDialogRequest::ExportPackage { .. }
        ));
        bridge.finish_file_dialog();
        assert!(!bridge.has_pending_file_dialog());
        driver.frame(Vec::new()).drop_without_applying_deltas();
        driver.frame(Vec::new()).drop_without_applying_deltas();
        assert_eq!(
            driver.editor.native_jump_mode.frame_navigation_owner(),
            first_owner
        );

        let stale_target = driver
            .editor
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .find(|target| {
                target.metadata().label() == "Ring"
                    && target.metadata().kind() == TargetKind::Action
            })
            .cloned()
            .expect("the owner recovers after the queued picker request is drained");
        let leader = egui::Modifiers::SHIFT;
        driver
            .frame_with_modifiers(
                vec![
                    key_event(egui::Key::Space, leader, true),
                    egui::Event::Text(" ".into()),
                ],
                leader,
            )
            .drop_without_applying_deltas();
        let stale_hint = driver
            .editor
            .native_jump_mode
            .active_session()
            .unwrap()
            .hint_map()
            .hint_for(stale_target.id())
            .expect("the old tree target has a current session assignment")
            .as_str()
            .to_owned();
        driver
            .frame_with_modifiers(vec![key_event(egui::Key::Space, leader, false)], leader)
            .drop_without_applying_deltas();

        driver.editor.force_close();
        assert!(driver.editor.session.is_none());
        assert!(driver.editor.native_jump_mode.active_session().is_none());
        assert!(
            driver
                .editor
                .native_jump_mode
                .retained_registry()
                .targets()
                .is_empty()
        );
        driver.editor.open_test_snapshot();
        driver.editor.tree_visible = true;
        driver.editor.session.as_mut().unwrap().font_catalog_loaded = true;
        driver.editor.viewport_close_pending = false;
        driver.frame.navigation_instance = Some(2);
        let reopened_baseline =
            std::sync::Arc::clone(&driver.editor.session.as_ref().unwrap().draft);
        let reopened_selection = driver.editor.session.as_ref().unwrap().selection.clone();
        driver.frame(Vec::new()).drop_without_applying_deltas();
        driver.frame(Vec::new()).drop_without_applying_deltas();
        let replacement_owner = NavigationOwner::Dialog {
            kind: NavigationDialogKind::RadialDesigner,
            instance: 2,
        };
        assert_eq!(
            driver.editor.native_jump_mode.frame_navigation_owner(),
            replacement_owner
        );
        let replacement_target = driver
            .editor
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .find(|target| {
                target.metadata().label() == "Ring"
                    && target.metadata().kind() == TargetKind::Action
            })
            .cloned()
            .expect("the reopened viewport collected its current Ring response");
        assert_ne!(replacement_target.id(), stale_target.id());
        assert_ne!(
            replacement_target.id().surface(),
            &SurfaceId::dialog(NavigationDialogKind::RadialDesigner.surface_kind(), 1)
        );
        assert!(driver.editor.native_jump_mode.active_session().is_none());

        // A stale hint string without its retired session/TargetId has no
        // authoring authority. The reopened document remains untouched.
        driver
            .frame(vec![egui::Event::Text(stale_hint)])
            .drop_without_applying_deltas();
        let replacement_session = driver.editor.session.as_ref().unwrap();
        assert!(!replacement_session.is_dirty());
        assert_eq!(replacement_session.pending_request, None);
        assert!(std::sync::Arc::ptr_eq(
            &replacement_session.draft,
            &reopened_baseline
        ));

        let fresh_leader = egui::Modifiers::SHIFT;
        driver
            .frame_with_modifiers(
                vec![
                    key_event(egui::Key::Space, fresh_leader, true),
                    egui::Event::Text(" ".into()),
                ],
                fresh_leader,
            )
            .drop_without_applying_deltas();
        let fresh_session = driver.editor.native_jump_mode.active_session().unwrap();
        assert!(
            fresh_session
                .hint_map()
                .hint_for(stale_target.id())
                .is_none()
        );
        assert!(
            fresh_session
                .hint_map()
                .hint_for(replacement_target.id())
                .is_some()
        );
        driver
            .frame_with_modifiers(
                vec![key_event(egui::Key::Space, fresh_leader, false)],
                fresh_leader,
            )
            .drop_without_applying_deltas();

        let (foreign_frame, focused_layer) =
            driver.frame_with_foreground_control(Vec::new(), egui::Modifiers::default());
        foreign_frame.drop_without_applying_deltas();
        assert_eq!(
            focused_layer.map(|layer| layer.order),
            Some(egui::Order::Foreground)
        );
        assert!(
            driver
                .editor
                .native_jump_mode
                .retained_registry()
                .targets()
                .is_empty()
        );
        let final_session = driver.editor.session.as_ref().unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &final_session.draft,
            &reopened_baseline
        ));
        assert_eq!(final_session.selection, reopened_selection);
    }

    #[test]
    fn jump_mode_radial_tree_foreign_foreground_focus_blocks_native_shortcuts() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor
            .session
            .as_mut()
            .expect("authoring session")
            .font_catalog_loaded = true;
        editor.make_dirty_for_test();
        let mut driver = RetainedDesignerDriver::new(editor);
        driver.frame.jump_mode_enabled = false;
        let before = {
            let session = driver.editor.session.as_ref().unwrap();
            (
                std::sync::Arc::clone(&session.draft),
                session.generation,
                session.last_error.clone(),
            )
        };

        let modifiers = egui::Modifiers::CTRL;
        let (focus_frame, _) =
            driver.frame_with_foreground_control(Vec::new(), egui::Modifiers::default());
        focus_frame.drop_without_applying_deltas();
        let (output, focused_layer_at_shortcut) = driver.frame_with_foreground_control(
            vec![key_event(egui::Key::S, modifiers, true)],
            modifiers,
        );
        output.drop_without_applying_deltas();
        assert_eq!(
            focused_layer_at_shortcut.map(|layer| layer.order),
            Some(egui::Order::Foreground),
            "the foreign response already owns focus before the shortcut is dispatched"
        );
        let session = driver.editor.session.as_ref().unwrap();
        assert!(std::sync::Arc::ptr_eq(&session.draft, &before.0));
        assert_eq!(session.generation, before.1);
        assert!(session.is_dirty());
        assert_eq!(session.last_error, before.2);
        assert_eq!(session.pending_request, None);
    }

    #[test]
    fn jump_mode_radial_children_properties_query_focus_types_then_guard_cancel_is_fresh() {
        use crate::gui::jump_mode::{SemanticTargetId, SurfaceId, TargetKind};

        let (mut driver, selection, initial) = retained_jump_query_properties_popup();
        initial.drop_without_applying_deltas();
        let original_document = std::sync::Arc::clone(
            &driver
                .editor
                .session
                .as_ref()
                .expect("authoring session")
                .draft,
        );
        let original_query = driver
            .editor
            .properties_draft
            .as_ref()
            .expect("Properties draft")
            .action_editor
            .query_for_test()
            .to_owned();
        assert!(original_query.is_empty());

        let query_target = driver
            .editor
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .find(|target| {
                target.metadata().label() == "Action query"
                    && target.metadata().kind() == TargetKind::TextEntry
                    && matches!(target.id().semantic(), SemanticTargetId::Named(_))
                    && matches!(target.id().surface(), SurfaceId::Popup { kind, owner, .. }
                        if kind == "radial-cell-properties"
                            && owner.as_ref() == &SurfaceId::dialog("radial-designer", 1))
            })
            .cloned()
            .expect("the current Properties query is a real child TextEdit target");
        assert!(query_target.eligible());
        assert!(query_target.clipped_rect().is_positive());
        assert_eq!(query_target.viewport(), radial_designer_viewport_id());

        let hint = start_retained_hint_session(&mut driver, &query_target);
        let (focused_frame, focus_after_hint) = complete_retained_hint_session(&mut driver, hint);
        focused_frame.drop_without_applying_deltas();
        assert_eq!(focus_after_hint, Some(query_target.response_id()));
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .query_for_test(),
            original_query
        );
        assert!(std::sync::Arc::ptr_eq(
            &driver.editor.session.as_ref().unwrap().draft,
            &original_document,
        ));
        assert_eq!(
            driver.editor.session.as_ref().unwrap().pending_request,
            None
        );

        let typed = driver.frame(vec![egui::Event::Text("λ query".into())]);
        typed.drop_without_applying_deltas();
        let mut focus_after_typing = None;
        let typed_again = driver.frame_with_observer(
            vec![egui::Event::Text(" second".into())],
            egui::Modifiers::default(),
            |ctx, _| focus_after_typing = ctx.memory(|memory| memory.focused()),
        );
        typed_again.drop_without_applying_deltas();
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .query_for_test(),
            "λ query second",
            "the native query editor accepts Unicode after focus-only completion"
        );
        assert_eq!(focus_after_typing, Some(query_target.response_id()));
        assert!(std::sync::Arc::ptr_eq(
            &driver.editor.session.as_ref().unwrap().draft,
            &original_document
        ));
        assert_eq!(
            driver.editor.session.as_ref().unwrap().pending_request,
            None
        );

        queue_navigation_to_screenshot_region(&mut driver);
        let transition = driver.frame(Vec::new());
        transition.drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_some());
        assert_eq!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&selection)
        );
        let guard_frame = driver.frame(Vec::new());
        guard_frame.drop_without_applying_deltas();
        // The guard’s first retained-layer frame is blocked while the shared
        // owner transition settles. Observe its controls on the next frame.
        let guard_owned_frame = driver.frame(Vec::new());
        guard_owned_frame.drop_without_applying_deltas();
        assert!(matches!(
            driver.editor.native_jump_mode.frame_navigation_owner(),
            NavigationOwner::RadialDesignerChild {
                kind: RadialDesignerChildKind::ScopeChangeGuard,
                ..
            }
        ));
        let cancel_target = driver
            .editor
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .find(|target| {
                target.metadata().label() == "Cancel navigation"
                    && target.metadata().kind() == TargetKind::Button
            })
            .cloned()
            .expect("the exact scope-change child exposes Cancel navigation");
        assert!(cancel_target.eligible());
        assert!(cancel_target.clipped_rect().is_positive());

        let hint = start_retained_hint_session(&mut driver, &cancel_target);
        let (focused_frame, focus_after_hint) = complete_retained_hint_session(&mut driver, hint);
        focused_frame.drop_without_applying_deltas();
        assert_eq!(focus_after_hint, Some(cancel_target.response_id()));
        assert!(driver.editor.pending_scope_change.is_some());
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .query_for_test(),
            "λ query second",
            "hint completion only focuses the guard button"
        );

        driver
            .frame(vec![key_event(
                egui::Key::Enter,
                egui::Modifiers::default(),
                true,
            )])
            .drop_without_applying_deltas();
        driver
            .frame(vec![key_event(
                egui::Key::Enter,
                egui::Modifiers::default(),
                false,
            )])
            .drop_without_applying_deltas();
        assert!(driver.editor.pending_scope_change.is_none());
        assert_eq!(
            driver.editor.session.as_ref().unwrap().selection.as_ref(),
            Some(&selection),
            "fresh Cancel retires the pending navigation without applying it"
        );
        assert_eq!(
            driver
                .editor
                .properties_draft
                .as_ref()
                .unwrap()
                .action_editor
                .query_for_test(),
            "λ query second",
            "Cancel keeps the unsaved Properties buffer"
        );
        assert!(std::sync::Arc::ptr_eq(
            &driver.editor.session.as_ref().unwrap().draft,
            &original_document,
        ));
        assert!(
            driver
                .editor
                .session
                .as_ref()
                .unwrap()
                .pending_request
                .is_none()
        );
    }

    #[test]
    fn jump_mode_radial_children_add_destination_popup_keeps_exact_child_scope() {
        use crate::gui::jump_mode::{SemanticTargetId, SurfaceId, TargetKind};

        let mut driver = retained_jump_radial_add_request();
        let request = driver
            .editor
            .radial_add_request
            .as_ref()
            .expect("pending Add-to-Radial request")
            .trace_identity
            .request_id;
        assert_eq!(request, 901);
        let original_document = std::sync::Arc::clone(
            &driver
                .editor
                .session
                .as_ref()
                .expect("authoring session")
                .draft,
        );
        let original_menu = driver.editor.radial_add_menu.clone();
        let parent_surface = SurfaceId::dialog("radial-designer", 1);
        let combo = driver
            .editor
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .find(|target| {
                target.metadata().label() == "Destination menu"
                    && target.metadata().kind() == TargetKind::ComboBox
                    && matches!(target.id().surface(), SurfaceId::Popup { kind, owner, .. }
                        if kind == "radial-add-to-menu" && owner.as_ref() == &parent_surface)
            })
            .cloned()
            .expect("the real destination combo belongs to the exact Add child");
        assert!(combo.eligible());
        assert!(combo.enabled());
        assert!(combo.clipped_rect().is_positive());

        let combo_point = combo.clipped_rect().center();
        driver
            .frame(vec![egui::Event::PointerMoved(combo_point)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(combo_point, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(combo_point, false)])
            .drop_without_applying_deltas();
        let mut combo_open_in_child = false;
        let sizing_or_open =
            driver.frame_with_observer(Vec::new(), egui::Modifiers::default(), |ctx, _| {
                combo_open_in_child = egui::ComboBox::is_open(ctx, combo.response_id());
            });
        sizing_or_open.drop_without_applying_deltas();
        assert!(combo_open_in_child, "the actual child ComboBox opened");

        let options = driver
            .editor
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .filter(|target| {
                target.metadata().label() == "Destination menu"
                    && target.metadata().kind() == TargetKind::Selectable
                    && matches!(target.id().semantic(), SemanticTargetId::Named(_))
            })
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            options.len() >= 2,
            "at least two actual menu choices are visible"
        );
        let option = options
            .iter()
            .find(|target| {
                matches!(target.id().surface(), SurfaceId::Popup { owner, .. }
                    if owner.as_ref() == combo.id().surface())
                    && driver
                        .editor
                        .session
                        .as_ref()
                        .unwrap()
                        .draft
                        .menus
                        .iter()
                        .filter(|menu| Some(&menu.id) != original_menu.as_ref())
                        .any(|menu| {
                            target_has_local_identity(
                                target,
                                &format!("radial-add/menu/{:016x}", radial_value_digest(&menu.id)),
                            )
                        })
            })
            .cloned()
            .expect("an option for another real menu is under the combo popup surface");
        assert!(option.eligible());
        assert!(option.enabled());
        assert!(option.clipped_rect().is_positive());
        assert_eq!(option.viewport(), radial_designer_viewport_id());
        assert_eq!(option.metadata().label(), "Destination menu");
        if let SemanticTargetId::Named(identity) = option.id().semantic() {
            assert!(!identity.contains("first argument"));
            assert!(!identity.contains("Close window"));
        }

        let hint = start_retained_hint_session(&mut driver, &option);
        let (focused_frame, focus_after_hint) = complete_retained_hint_session(&mut driver, hint);
        focused_frame.drop_without_applying_deltas();
        assert_eq!(focus_after_hint, Some(option.response_id()));
        assert_eq!(driver.editor.radial_add_menu, original_menu);
        assert!(driver.editor.radial_add_request.is_some());
        assert_eq!(
            driver.editor.session.as_ref().unwrap().pending_request,
            None
        );
        assert!(std::sync::Arc::ptr_eq(
            &driver.editor.session.as_ref().unwrap().draft,
            &original_document
        ));
    }

    #[test]
    fn jump_mode_radial_children_appearance_choice_is_focus_only() {
        use crate::gui::jump_mode::{SemanticTargetId, SurfaceId, TargetKind};

        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.show_resources = true;
        let destination = {
            let session = editor.session.as_mut().expect("authoring session");
            session.font_catalog_loaded = true;
            let destination = session.draft.default_menu_id.clone();
            session.select(Some(StableSelection::Menu(destination.clone())));
            destination
        };
        editor
            .appearance
            .prepare_choice_for_test(editor.session.as_ref().unwrap(), destination.clone(), 0)
            .expect("the current gallery choice is a valid prepared candidate");
        let original_document =
            std::sync::Arc::clone(&editor.session.as_ref().expect("authoring session").draft);
        let original_choice_key = editor
            .appearance
            .choice_key_digest_for_test()
            .expect("the selected gallery tile has a key");
        assert_eq!(
            editor.appearance.entry_key_digest_for_test(0),
            Some(original_choice_key)
        );
        let mut driver = RetainedDesignerDriver::new(editor);
        driver.frame.jump_mode_enabled = true;
        driver.frame(Vec::new()).drop_without_applying_deltas();
        let initial = driver.frame(Vec::new());
        initial.drop_without_applying_deltas();
        let original_preview_menu = driver.editor.preview.current_menu().cloned();
        let original_candidate = driver
            .editor
            .appearance
            .candidate(driver.editor.session.as_ref().unwrap())
            .expect("the prepared candidate survives the initial real viewport pass");
        let tile = driver
            .editor
            .native_jump_mode
            .retained_registry()
            .targets()
            .iter()
            .find(|target| {
                target.metadata().label() == "Appearance choice"
                    && target.metadata().kind() == TargetKind::Selectable
                    && matches!(target.id().semantic(), SemanticTargetId::Named(_))
                    && matches!(target.id().surface(), SurfaceId::Dialog { kind, instance }
                        if kind == "radial-designer" && *instance == 1)
                    && !target_has_local_identity(
                        target,
                        &format!("appearance/tile/{original_choice_key:016x}"),
                    )
            })
            .cloned()
            .expect("an alternate real gallery tile is visible and targetable");
        assert!(tile.eligible());
        assert!(tile.enabled());
        assert!(tile.clipped_rect().is_positive());
        assert_eq!(tile.viewport(), radial_designer_viewport_id());

        let hint = start_retained_hint_session(&mut driver, &tile);
        let (focused_frame, focus_after_hint) = complete_retained_hint_session(&mut driver, hint);
        focused_frame.drop_without_applying_deltas();
        assert_eq!(focus_after_hint, Some(tile.response_id()));
        let session = driver.editor.session.as_ref().expect("authoring session");
        let candidate_after = driver
            .editor
            .appearance
            .candidate(session)
            .expect("the same prepared gallery candidate remains available");
        assert!(std::sync::Arc::ptr_eq(&session.draft, &original_document));
        assert_eq!(session.selection, Some(StableSelection::Menu(destination)));
        assert_eq!(session.pending_request, None);
        assert!(!session.is_dirty());
        assert_eq!(
            driver.editor.appearance.choice_key_digest_for_test(),
            Some(original_choice_key),
            "hint completion focuses a tile without choosing it"
        );
        assert!(std::sync::Arc::ptr_eq(
            &candidate_after.0,
            &original_candidate.0
        ));
        assert_eq!(candidate_after.1, original_candidate.1);
        assert_eq!(
            driver.editor.preview.current_menu().cloned(),
            original_preview_menu,
            "the actual preview destination stays unchanged"
        );
    }

    #[test]
    fn headless_accesskit_tree_names_a_completely_blank_cell_by_stable_id() {
        let mut document = RadialDocument::starter();
        let cell = &mut document.menus[0].rings[0].cells[0];
        cell.label.clear();
        cell.tooltip = crate::radial::model::Override::Inherit;
        cell.icon = crate::radial::model::Override::Inherit;
        let expected = format!("Cell 1 ({})", cell.id);
        let mut editor = RadialEditorState::default();
        editor.session = Some(RadialAuthoringSession::new(AuthoringSnapshot::new(
            std::sync::Arc::new(document),
            "test",
        )));
        let menu_id = editor.session.as_ref().unwrap().draft.menus[0].id.clone();
        editor
            .preferences
            .expanded_sections
            .insert(format!("menu:{menu_id}"), true);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                editor.tree(
                    ui,
                    &crate::radial::model::RadialFeatureSettings::default(),
                    ViewportClass::Deferred,
                    &RadialUiPass::default(),
                );
            });
        });
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("AccessKit tree update");
        assert!(update.nodes.iter().any(|(_, node)| {
            node.role() == egui::accesskit::Role::Button && node.label() == Some(expected.as_str())
        }));
        output.drop_without_applying_deltas();
    }

    #[test]
    fn retained_filtered_tree_exposes_every_selected_cell_membership_to_accesskit() {
        use crate::radial::authoring::AuthoredCellTarget;

        let mut document = RadialDocument::starter();
        let menu = &mut document.menus[0];
        let ring = &mut menu.rings[0];
        let labels = [
            "selection-match-first",
            "selection-match-second",
            "selection-match-unselected",
        ];
        for (cell, label) in ring.cells.iter_mut().zip(labels) {
            cell.label = label.into();
        }
        let targets = ring.cells[..2]
            .iter()
            .map(|cell| AuthoredCellTarget {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: cell.id.clone(),
            })
            .collect::<Vec<_>>();
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let mut session = RadialAuthoringSession::new(AuthoringSnapshot::new(
            std::sync::Arc::new(document),
            "filtered-accesskit-membership",
        ));
        session.select_authored_cell(targets[0].clone(), false, false);
        session.select_authored_cell(targets[1].clone(), true, false);
        assert!(matches!(
            session.selection,
            Some(StableSelection::CellSet(_))
        ));
        let before = (
            session.draft.clone(),
            session.generation,
            session.selection.clone(),
            session.acceptance_history_depths(),
            session.pending_assets.clone(),
        );
        editor.session = Some(session);
        editor.tree_visible = true;
        editor.inspector_visible = false;
        editor.tree_filter = "selection-match".into();
        let mut driver = RetainedDesignerDriver::new(editor);
        driver.frame(Vec::new()).drop_without_applying_deltas();
        let output = driver.frame(Vec::new());
        let update = output.platform_output.accesskit_update.as_ref().unwrap();
        for (label, expected) in labels.into_iter().zip([true, true, false]) {
            let matching = update
                .nodes
                .iter()
                .filter(|(_, node)| {
                    node.role() == egui::accesskit::Role::Button
                        && node.label().is_some_and(|name| name.contains(label))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                matching.len(),
                1,
                "one semantic filtered result for {label}"
            );
            assert_eq!(
                matching[0].1.toggled(),
                Some(if expected {
                    egui::accesskit::Toggled::True
                } else {
                    egui::accesskit::Toggled::False
                }),
                "membership for {label}"
            );
        }
        let session = driver.editor.session.as_ref().unwrap();
        assert_eq!(
            (
                session.draft.clone(),
                session.generation,
                session.selection.clone(),
                session.acceptance_history_depths(),
                session.pending_assets.clone(),
            ),
            before,
        );
        assert!(driver.editor.pending_scope_change.is_none());
        assert_eq!(driver.editor.tree_filter, "selection-match");
    }

    #[test]
    fn resource_notice_severity_keeps_success_and_portability_non_error_in_accesskit() {
        let notices = [
            ResourceNotice::info("Exported package"),
            ResourceNotice::info("Managed and portable"),
            ResourceNotice::warning("External path; excluded from portable packages"),
            ResourceNotice::error("Decode failed"),
        ];
        assert_eq!(notices[0].severity, ResourceNoticeSeverity::Info);
        assert_eq!(notices[1].severity, ResourceNoticeSeverity::Info);
        assert_eq!(notices[2].severity, ResourceNoticeSeverity::Warning);
        assert_eq!(notices[3].severity, ResourceNoticeSeverity::Error);

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.begin_pass(egui::RawInput::default());
        let mut root_ui = egui::Ui::new(
            ctx.clone(),
            egui::Id::new("resource-notice-accesskit-root"),
            egui::UiBuilder::new().max_rect(ctx.content_rect()),
        );
        egui::CentralPanel::default().show(&mut root_ui, |ui| {
            for notice in &notices {
                show_resource_notice(ui, notice);
            }
        });
        let output = ctx.end_pass();
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("AccessKit tree update");
        let names = update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::accesskit::Role::Label)
            .filter_map(|(_, node)| node.value())
            .collect::<Vec<_>>();
        assert!(names.contains(&"Info: Exported package"));
        assert!(names.contains(&"Info: Managed and portable"));
        assert!(
            names
                .iter()
                .any(|name| name.starts_with("Warning: External path"))
        );
        assert!(names.contains(&"Error: Decode failed"));
        output.drop_without_applying_deltas();
    }

    #[test]
    fn designer_submission_edge_is_bounded_to_session_and_generation_changes() {
        let mut previous = None;
        assert!(mark_trace_submission(&mut previous, Some((4, 1))));
        assert!(!mark_trace_submission(&mut previous, Some((4, 1))));
        assert!(mark_trace_submission(&mut previous, Some((4, 2))));
        assert!(mark_trace_submission(&mut previous, Some((5, 1))));
        assert!(!mark_trace_submission(&mut previous, Some((5, 1))));
        assert!(!mark_trace_submission(&mut previous, None));
        assert!(!mark_trace_submission(&mut previous, None));
        assert!(mark_trace_submission(&mut previous, Some((5, 2))));
    }

    #[test]
    fn keyboard_only_undo_redo_routes_unconsumed_editor_shortcuts() {
        let document = RadialDocument::starter();
        let original = document.menus[0].name.clone();
        let menu_id = document.menus[0].id.clone();
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut editor = RadialEditorState::default();
        editor.session = Some(RadialAuthoringSession::new(snapshot));
        menu::rename_menu(
            editor.session.as_mut().unwrap(),
            menu_id,
            "Keyboard edit".into(),
            EditPhase::Atomic,
        )
        .unwrap();

        let key = |modifiers| egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput {
            events: vec![
                key(egui::Modifiers::CTRL),
                egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
            ],
            ..Default::default()
        });
        editor.keyboard_shortcuts(&ctx);
        let undo = editor
            .pending_scope_change
            .take()
            .expect("Ctrl+Z queues an undo intent");
        assert!(matches!(
            &undo.intent,
            ScopeChangeIntent::Operation(DeferredEditorOperation::Undo)
        ));
        editor.apply_pending_scope_change(undo);
        assert_eq!(
            editor.session.as_ref().unwrap().draft.menus[0].name,
            original
        );
        assert!(!ctx.input(|input| input.key_pressed(egui::Key::Z)));
        ctx.end_pass().drop_without_applying_deltas();

        ctx.begin_pass(egui::RawInput {
            events: vec![
                key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT),
                egui::Event::ModifiersChanged(egui::Modifiers::CTRL | egui::Modifiers::SHIFT),
            ],
            ..Default::default()
        });
        editor.keyboard_shortcuts(&ctx);
        let redo = editor
            .pending_scope_change
            .take()
            .expect("Ctrl+Shift+Z queues a redo intent");
        assert!(matches!(
            &redo.intent,
            ScopeChangeIntent::Operation(DeferredEditorOperation::Redo)
        ));
        editor.apply_pending_scope_change(redo);
        assert_eq!(
            editor.session.as_ref().unwrap().draft.menus[0].name,
            "Keyboard edit"
        );
        ctx.end_pass().drop_without_applying_deltas();
    }

    #[test]
    fn retained_tree_search_keeps_text_shortcuts_local_and_escape_clears_only_the_filter() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.make_dirty_for_test();
        editor.tree_visible = true;
        editor.inspector_visible = false;
        editor.tree_filter = "no-match".into();
        let initial_selection = {
            let session = editor.session.as_mut().expect("authoring session");
            let menu = &session.draft.menus[0];
            let ring = &menu.rings[0];
            let cell = &ring.cells[0];
            let selection = StableSelection::Cell {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: cell.id.clone(),
            };
            session.select(Some(selection.clone()));
            selection
        };
        let (draft, generation, history_depths) = {
            let session = editor.session.as_ref().expect("authoring session");
            assert!(session.is_dirty());
            (
                std::sync::Arc::clone(&session.draft),
                session.generation,
                session.acceptance_history_depths(),
            )
        };
        let mut driver = RetainedDesignerDriver::new(editor);

        let initial = driver.frame(Vec::new());
        let search = accesskit_value_bounds(&initial, egui::accesskit::Role::TextInput, "no-match");
        let click = search.center();
        initial.drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::PointerMoved(click)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(click, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(click, false)])
            .drop_without_applying_deltas();
        let control = egui::Modifiers::CTRL.plus(egui::Modifiers::COMMAND);
        driver
            .frame_with_modifiers(vec![key_event(egui::Key::A, control, true)], control)
            .drop_without_applying_deltas();
        driver
            .frame_with_modifiers(vec![key_event(egui::Key::A, control, false)], control)
            .drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::Text("starter".into())])
            .drop_without_applying_deltas();
        assert_eq!(driver.editor.tree_filter, "starter");

        // Shift+Left selects the last character inside the focused TextEdit;
        // typing replaces it without changing the authored selection.
        driver
            .frame_with_modifiers(
                vec![key_event(
                    egui::Key::ArrowLeft,
                    egui::Modifiers::SHIFT,
                    true,
                )],
                egui::Modifiers::SHIFT,
            )
            .drop_without_applying_deltas();
        driver
            .frame_with_modifiers(
                vec![key_event(
                    egui::Key::ArrowLeft,
                    egui::Modifiers::SHIFT,
                    false,
                )],
                egui::Modifiers::SHIFT,
            )
            .drop_without_applying_deltas();
        driver
            .frame(vec![egui::Event::Text("x".into())])
            .drop_without_applying_deltas();
        assert_eq!(driver.editor.tree_filter, "startex");

        // Text undo is local to the filter even though the document has an
        // older dirty edit that a Designer-level Ctrl+Z could otherwise undo.
        driver
            .frame_with_modifiers(vec![key_event(egui::Key::Z, control, true)], control)
            .drop_without_applying_deltas();
        assert_eq!(driver.editor.tree_filter, "no-match");
        driver
            .frame(vec![key_event(
                egui::Key::Escape,
                egui::Modifiers::default(),
                true,
            )])
            .drop_without_applying_deltas();

        let session = driver.editor.session.as_ref().expect("authoring session");
        assert!(driver.editor.open, "Escape clears the local filter first");
        assert!(driver.editor.tree_filter.is_empty());
        assert!(session.is_dirty());
        assert_eq!(session.generation, generation);
        assert!(std::sync::Arc::ptr_eq(&session.draft, &draft));
        assert_eq!(session.selection.as_ref(), Some(&initial_selection));
        assert_eq!(session.acceptance_history_depths(), history_depths);
    }

    #[test]
    fn explicit_generated_result_source_edit_selects_only_the_authored_source() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.tree_visible = false;
        editor.inspector_visible = true;
        let (source_selection, source, fingerprint, draft, generation, history_depths) = {
            let session = editor.session.as_mut().expect("authoring session");
            let menu = session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == MenuId::new("starter-favorites"))
                .expect("starter dynamic source menu");
            let ring = menu.rings.first().expect("source ring");
            let source_cell = ring
                .cells
                .iter()
                .find(|cell| matches!(cell.content, CellContent::Dynamic { .. }))
                .expect("authored dynamic source cell");
            let CellContent::Dynamic { source } = &source_cell.content else {
                unreachable!("filtered dynamic source cell")
            };
            let source = source.clone();
            let source_selection = StableSelection::Cell {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: source_cell.id.clone(),
            };
            (
                source_selection,
                source,
                crate::radial::dynamic::SourceFingerprint {
                    generation: 17,
                    source: "headless-generated-result".into(),
                    query: None,
                },
                std::sync::Arc::clone(&session.draft),
                session.generation,
                session.acceptance_history_depths(),
            )
        };
        editor.projected_selection = Some(ProjectedSelection {
            label: "Generated favorite result".into(),
            provenance: ProjectedCellProvenance::Dynamic {
                menu_id: match &source_selection {
                    StableSelection::Cell { menu_id, .. } => menu_id.clone(),
                    _ => unreachable!("source selection is a cell"),
                },
                ring_id: match &source_selection {
                    StableSelection::Cell { ring_id, .. } => ring_id.clone(),
                    _ => unreachable!("source selection is a cell"),
                },
                source_cell_id: match &source_selection {
                    StableSelection::Cell { cell_id, .. } => cell_id.clone(),
                    _ => unreachable!("source selection is a cell"),
                },
                source,
                result_index: 0,
                fingerprint,
            },
        });
        let mut driver = RetainedDesignerDriver::new(editor);

        let initial = driver.frame(Vec::new());
        let button = accesskit_named_bounds(
            &initial,
            "Edit dynamic source definition",
            egui::accesskit::Role::Button,
        );
        initial.drop_without_applying_deltas();
        let click = button.center();
        driver
            .frame(vec![egui::Event::PointerMoved(click)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(click, true)])
            .drop_without_applying_deltas();
        driver
            .frame(vec![pointer_button_event(click, false)])
            .drop_without_applying_deltas();

        let session = driver.editor.session.as_ref().expect("authoring session");
        assert!(driver.editor.projected_selection.is_none());
        assert_eq!(session.selection.as_ref(), Some(&source_selection));
        assert!(source_selection.exists_in(&session.draft));
        assert!(std::sync::Arc::ptr_eq(&session.draft, &draft));
        assert_eq!(session.generation, generation);
        assert!(!session.is_dirty());
        assert_eq!(session.acceptance_history_depths(), history_depths);
    }
}
