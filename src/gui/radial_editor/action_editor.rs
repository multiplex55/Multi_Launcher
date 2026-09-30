//! Query-first Universal Action editor shared by Cell Properties and Inspector.

use crate::radial::authoring::{AuthoringSessionId, DraftGeneration, StableSelection};
use crate::radial::context::InvocationContext;
use crate::radial::model::{ActionBinding, QueryRunMode};
use eframe::egui;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(160);
const SEARCH_RETRY_BASE: Duration = Duration::from_millis(160);
const SEARCH_RETRY_MAX: Duration = Duration::from_secs(2);
const RESULT_SCROLL_MAX_HEIGHT: f32 = 190.0;
static NEXT_EDITOR_EPOCH: AtomicU64 = AtomicU64::new(1);

fn in_result_scroll_column<R>(
    ui: &mut egui::Ui,
    reserve_parent_scrollbar: bool,
    column_width_at_query_start: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    if !reserve_parent_scrollbar {
        return add_contents(ui);
    }

    let available = ui.available_size();
    let scroll = ui.spacing().scroll;
    let parent_scrollbar_hit_width = scroll.bar_width.max(scroll.allocated_width())
        + scroll.bar_inner_margin
        + scroll.bar_outer_margin;
    let list_width = available.x.min(column_width_at_query_start);
    let allocated = ui.allocate_ui_with_layout(
        egui::vec2(
            (list_width - parent_scrollbar_hit_width).max(0.0),
            available.y.min(RESULT_SCROLL_MAX_HEIGHT),
        ),
        egui::Layout::top_down(egui::Align::Min),
        add_contents,
    );
    allocated.inner
}

fn next_editor_epoch() -> u64 {
    NEXT_EDITOR_EPOCH.fetch_add(1, Ordering::Relaxed).max(1)
}

fn trace_private_digest(parts: &[&str]) -> u64 {
    crate::radial::acceptance_trace::private_trace_parts_digest(parts)
}

fn trace_private_digest_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn trace_action_editor_scroll(
    ui: &egui::Ui,
    output: &egui::scroll_area::ScrollAreaOutput<()>,
    identity: &AuthoringBindingEditorIdentity,
    query: &str,
) {
    if !crate::radial::acceptance_trace::enabled() && !cfg!(test) {
        return;
    }
    let pixels_per_point = ui.ctx().pixels_per_point();
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return;
    }
    let Some(viewport_rect) = ui.ctx().input(|input| input.viewport().inner_rect) else {
        return;
    };
    let client_size = [viewport_rect.width(), viewport_rect.height()];
    let client_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_rect.size());
    let Some(track_response) = ui.ctx().read_response(output.id.with(1)) else {
        return;
    };
    if track_response.id != output.id.with(1) {
        return;
    }
    let inner_height = output.inner_rect.height();
    let content_height = output.content_size.y;
    let offset = output.state.offset.y;
    let offset_end = offset + inner_height;
    let handle_min_length = ui.style().spacing.scroll.handle_min_length;
    if !inner_height.is_finite()
        || !content_height.is_finite()
        || !offset.is_finite()
        || !offset_end.is_finite()
        || !handle_min_length.is_finite()
        || content_height <= inner_height
        || inner_height <= 0.0
        || handle_min_length <= 0.0
        || offset < 0.0
        || offset_end > content_height + f32::EPSILON
    {
        return;
    }
    let track = track_response.rect;
    let track_visible = track
        .intersect(track_response.interact_rect)
        .intersect(ui.clip_rect())
        .intersect(client_rect);
    let inner_visible = output
        .inner_rect
        .intersect(ui.clip_rect())
        .intersect(client_rect);
    if !track.is_positive() || !track_visible.is_positive() || !inner_visible.is_positive() {
        return;
    }
    // egui 0.27 maps the vertical handle to the inner rect's y range, with its
    // y coordinates remapped from content offsets. The bar response owns the
    // cross-axis bounds; reading it after ScrollArea::show gives the actual
    // current frame's track geometry, including floating-bar expansion.
    let content_to_track = |content_y: f32| {
        output.inner_rect.top()
            + (content_y / content_height).clamp(0.0, 1.0) * output.inner_rect.height()
    };
    let thumb = egui::Rect::from_min_max(
        egui::pos2(track.left(), content_to_track(offset)),
        egui::pos2(track.right(), content_to_track(offset_end)),
    );
    // ScrollArea paints a minimum-length handle centered on the raw mapping.
    // Preserve the raw thumb separately because that is the portion whose
    // center retains egui's content-offset mapping during a checked drag.
    let painted_thumb = if thumb.height() < handle_min_length {
        egui::Rect::from_center_size(thumb.center(), egui::vec2(thumb.width(), handle_min_length))
    } else {
        thumb
    };
    let paint_clip = ui.clip_rect().intersect(client_rect);
    let thumb_visible = thumb
        .intersect(track_visible)
        .intersect(ui.clip_rect())
        .intersect(client_rect);
    let painted_thumb_visible = painted_thumb.intersect(paint_clip);
    if !painted_thumb.is_positive()
        || !painted_thumb_visible.is_positive()
        || !paint_clip.is_positive()
    {
        return;
    }
    let to_pixels = |rect: egui::Rect| -> Option<[i32; 4]> {
        let scale = |coordinate: f32| {
            let pixel = coordinate * pixels_per_point;
            (pixel.is_finite() && pixel >= i32::MIN as f32 && pixel <= i32::MAX as f32)
                .then(|| pixel.round() as i32)
        };
        Some([
            scale(rect.left())?,
            scale(rect.top())?,
            scale(rect.right())?,
            scale(rect.bottom())?,
        ])
    };
    let to_milli_points = |value: f32| -> Option<i64> {
        let fixed = f64::from(value) * 1_000.0;
        (fixed.is_finite() && fixed >= i64::MIN as f64 && fixed <= i64::MAX as f64)
            .then(|| fixed.round() as i64)
    };
    let Some(inner_bounds) = to_pixels(output.inner_rect) else {
        return;
    };
    let Some(inner_visible_bounds) = to_pixels(inner_visible) else {
        return;
    };
    let Some(track_bounds) = to_pixels(track) else {
        return;
    };
    let Some(track_visible_bounds) = to_pixels(track_visible) else {
        return;
    };
    let Some(thumb_bounds) = to_pixels(thumb) else {
        return;
    };
    let Some(thumb_visible_bounds) = to_pixels(thumb_visible) else {
        return;
    };
    let Some(painted_thumb_bounds) = to_pixels(painted_thumb) else {
        return;
    };
    let Some(painted_thumb_visible_bounds) = to_pixels(painted_thumb_visible) else {
        return;
    };
    let Some(paint_clip_bounds) = to_pixels(paint_clip) else {
        return;
    };
    let client_width_px = (client_size[0] * pixels_per_point).round();
    let client_height_px = (client_size[1] * pixels_per_point).round();
    if !client_width_px.is_finite()
        || !client_height_px.is_finite()
        || client_width_px < 1.0
        || client_height_px < 1.0
        || client_width_px > i32::MAX as f32
        || client_height_px > i32::MAX as f32
    {
        return;
    }
    let client_width_px = client_width_px as i32;
    let client_height_px = client_height_px as i32;
    let Some(offset_y_milli) = to_milli_points(offset) else {
        return;
    };
    let Some(velocity_y_milli) = to_milli_points(output.state.velocity().y) else {
        return;
    };
    let Some(content_height_milli) = to_milli_points(content_height) else {
        return;
    };
    let Some(inner_height_milli) = to_milli_points(inner_height) else {
        return;
    };
    let Some(pixels_per_point_milli) = to_milli_points(pixels_per_point) else {
        return;
    };
    let Some(handle_min_length_milli) = to_milli_points(handle_min_length) else {
        return;
    };
    crate::radial::acceptance_trace::emit_action_editor_scroll(
        identity,
        crate::radial::acceptance_trace::ActionEditorScrollObservation {
            query_digest: crate::radial::acceptance_trace::private_trace_text_digest(query),
            scroll_id: output.id.with(1).value(),
            frame_nr: ui.ctx().frame_nr(),
            offset_y_milli,
            velocity_y_milli,
            content_height_milli,
            inner_height_milli,
            pixels_per_point_milli,
            handle_min_length_milli,
            inner_bounds,
            inner_visible_bounds,
            track_bounds,
            track_visible_bounds,
            thumb_bounds,
            thumb_visible_bounds,
            painted_thumb_bounds,
            painted_thumb_visible_bounds,
            paint_clip_bounds,
            client_size: [client_width_px, client_height_px],
        },
    );
}

pub(crate) fn trace_binding_digest(binding: Option<&ActionBinding>) -> u64 {
    binding
        .map(crate::radial::acceptance_trace::private_trace_serialized_digest)
        .unwrap_or(0)
}

fn trace_action_editor_control(
    ui: &egui::Ui,
    response: &egui::Response,
    identity: &AuthoringBindingEditorIdentity,
    control: &'static str,
    control_index: Option<usize>,
    target_digest: u64,
    action_digest: u64,
    binding: Option<&ActionBinding>,
    query: &str,
    enabled: bool,
    selected: bool,
) {
    trace_action_editor_control_with_presentation(
        ui,
        response,
        identity,
        control,
        control_index,
        target_digest,
        [0; 3],
        action_digest,
        binding,
        query,
        enabled,
        selected,
        response.changed(),
        false,
    );
}

fn trace_action_editor_control_with_presentation(
    ui: &egui::Ui,
    response: &egui::Response,
    identity: &AuthoringBindingEditorIdentity,
    control: &'static str,
    control_index: Option<usize>,
    target_digest: u64,
    presentation_digests: [u64; 3],
    action_digest: u64,
    binding: Option<&ActionBinding>,
    query: &str,
    enabled: bool,
    selected: bool,
    changed: bool,
    enter_pressed: bool,
) {
    trace_action_editor_control_with_full_text(
        ui,
        response,
        identity,
        control,
        control_index,
        target_digest,
        presentation_digests,
        action_digest,
        binding,
        query,
        enabled,
        selected,
        changed,
        enter_pressed,
        None,
    );
}

fn trace_action_editor_control_with_full_text(
    ui: &egui::Ui,
    response: &egui::Response,
    identity: &AuthoringBindingEditorIdentity,
    control: &'static str,
    control_index: Option<usize>,
    target_digest: u64,
    presentation_digests: [u64; 3],
    action_digest: u64,
    binding: Option<&ActionBinding>,
    query: &str,
    enabled: bool,
    selected: bool,
    changed: bool,
    enter_pressed: bool,
    displayed_text: Option<&str>,
) {
    if !crate::radial::acceptance_trace::enabled() && !cfg!(test) {
        return;
    }
    let pixels_per_point = ui.ctx().pixels_per_point();
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return;
    }
    let full = response.rect;
    let clipped = full.intersect(ui.clip_rect());
    if !clipped.is_positive() {
        return;
    }
    let scale = |coordinate: f32| {
        let value = coordinate * pixels_per_point;
        (value.is_finite() && value >= i32::MIN as f32 && value <= i32::MAX as f32)
            .then(|| value.round() as i32)
    };
    let Some(left) = scale(clipped.left()) else {
        return;
    };
    let Some(top) = scale(clipped.top()) else {
        return;
    };
    let Some(right) = scale(clipped.right()) else {
        return;
    };
    let Some(bottom) = scale(clipped.bottom()) else {
        return;
    };
    let Some(full_left) = scale(full.left()) else {
        return;
    };
    let Some(full_top) = scale(full.top()) else {
        return;
    };
    let Some(full_right) = scale(full.right()) else {
        return;
    };
    let Some(full_bottom) = scale(full.bottom()) else {
        return;
    };
    let Some(viewport_rect) = ui.ctx().input(|input| input.viewport().inner_rect) else {
        return;
    };
    let Some(client_width) = scale(viewport_rect.width()) else {
        return;
    };
    let Some(client_height) = scale(viewport_rect.height()) else {
        return;
    };
    // `response.rect` and `ui.clip_rect()` are client-local points. In egui
    // 0.27, `ViewportInfo::inner_rect` is monitor-space, so only its size is
    // comparable here; using its monitor-space origin hides every row when
    // the Designer is not at (0, 0).
    let client_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_rect.size());
    let fully_visible = action_editor_control_is_fully_visible(full, clipped, client_rect);
    let binding_digest = trace_binding_digest(binding);
    let query_digest = crate::radial::acceptance_trace::private_trace_text_digest(query);
    let value_digest = match (control, binding) {
        ("query_field", _) => query_digest,
        ("exact_command_field", Some(ActionBinding::ExactCommand { command, .. })) => {
            crate::radial::acceptance_trace::private_trace_text_digest(command)
        }
        ("exact_args_field", Some(ActionBinding::ExactCommand { args, .. })) => {
            crate::radial::acceptance_trace::private_trace_text_digest(
                args.as_deref().unwrap_or(""),
            )
        }
        _ => 0,
    };
    crate::radial::acceptance_trace::emit_action_editor_control(
        identity,
        control,
        control_index,
        target_digest,
        presentation_digests[0],
        presentation_digests[1],
        presentation_digests[2],
        action_digest,
        binding_digest,
        query_digest,
        value_digest,
        [left, top, right, bottom],
        [full_left, full_top, full_right, full_bottom],
        [client_width, client_height],
        displayed_text
            .map(crate::radial::acceptance_trace::private_trace_text_digest)
            .unwrap_or(0),
        fully_visible,
        enabled,
        selected,
        response.has_focus(),
        response.clicked(),
        changed,
        enter_pressed,
    );
}

fn action_editor_control_is_fully_visible(
    full: egui::Rect,
    clipped: egui::Rect,
    client_rect: egui::Rect,
) -> bool {
    full == clipped && client_rect.contains_rect(full)
}

#[derive(Clone, Copy)]
pub(crate) struct PickerRowTraceIdentity {
    pub(crate) target: u64,
    pub(crate) title: u64,
    pub(crate) target_type: u64,
    pub(crate) disambiguator: u64,
    pub(crate) action: u64,
    pub(crate) binding: u64,
}

pub(crate) fn trace_picker_row_identity(
    row: &crate::gui::universal_action_catalog::UniversalActionPickerRow,
) -> PickerRowTraceIdentity {
    PickerRowTraceIdentity {
        target: trace_private_digest(&[
            &row.target_command,
            &row.target_title,
            &row.target_type,
            &row.target_disambiguator,
        ]),
        title: crate::radial::acceptance_trace::private_trace_text_digest(&row.target_title),
        target_type: crate::radial::acceptance_trace::private_trace_text_digest(&row.target_type),
        disambiguator: crate::radial::acceptance_trace::private_trace_text_digest(
            &row.target_disambiguator,
        ),
        action: trace_private_digest(&[row.action_id.as_str()]),
        binding: trace_binding_digest(row.binding.as_ref()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BindingEditorSurface {
    Properties,
    Inspector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BindingEditorSlot {
    CellPrimary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingAssignmentKind {
    Explicit,
    Pinned,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BindingEditorScope {
    pub(crate) surface: BindingEditorSurface,
    pub(crate) editor_session: AuthoringSessionId,
    pub(crate) draft_generation: DraftGeneration,
    pub(crate) target: StableSelection,
    pub(crate) slot: BindingEditorSlot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthoringBindingEditorIdentity {
    pub(crate) scope: BindingEditorScope,
    /// Stable digest of the binding currently assigned to the edited cell.
    /// Each rendered control keeps its own query/result binding digest.
    pub(crate) assigned_binding_digest: u64,
    /// Changes whenever this editor instance visits a new scope or binding.
    /// This prevents late work from matching a later visit to the same cell.
    pub(crate) editor_epoch: u64,
    pub(crate) edit_generation: u64,
    pub(crate) query_generation: u64,
    pub(crate) query_request_generation: u64,
    pub(crate) search_request_generation: u64,
    pub(crate) test_request_generation: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum EditorTab {
    #[default]
    Query,
    Advanced,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum SearchStatus {
    #[default]
    Idle,
    Pending,
    Results,
    NoResults,
    Unavailable(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ActionBindingEditorState {
    scope: Option<BindingEditorScope>,
    active_binding: Option<ActionBinding>,
    query: String,
    query_mode: QueryRunMode,
    command: String,
    args: String,
    tab: EditorTab,
    search_status: SearchStatus,
    search_rows: Vec<crate::gui::universal_action_catalog::UniversalActionPickerRow>,
    search_preview: Option<String>,
    search_preview_unavailable: Option<String>,
    contextual_cache: Option<(
        InvocationContext,
        Vec<crate::gui::universal_action_catalog::UniversalActionPickerRow>,
    )>,
    selected_binding: Option<ActionBinding>,
    selected_binding_input_digest: Option<u64>,
    pinned_search_input: Option<(String, QueryRunMode)>,
    editor_epoch: u64,
    edit_generation: u64,
    query_generation: u64,
    query_request_generation: u64,
    search_request_generation: u64,
    test_request_generation: u64,
    pending_search_identity: Option<AuthoringBindingEditorIdentity>,
    pending_search_query: Option<String>,
    pending_search_due: Option<Instant>,
    search_retry_attempts: u8,
    pending_test_binding: Option<ActionBinding>,
    pending_test_identity: Option<AuthoringBindingEditorIdentity>,
    pending_test_query: Option<String>,
    test_status: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum ActionBindingEditorIntent {
    Search {
        identity: AuthoringBindingEditorIdentity,
        query: String,
    },
    Pin {
        binding: ActionBinding,
    },
    SaveQuery {
        binding: ActionBinding,
    },
    SetCommand {
        binding: ActionBinding,
    },
    Test {
        identity: AuthoringBindingEditorIdentity,
        binding: ActionBinding,
        history_query: String,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ActionSearchCompletion {
    pub(crate) identity: AuthoringBindingEditorIdentity,
    pub(crate) state: ActionSearchCompletionState,
    pub(crate) rows: Vec<crate::gui::universal_action_catalog::UniversalActionPickerRow>,
    pub(crate) preview: Option<String>,
    pub(crate) preview_unavailable: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum ActionSearchCompletionState {
    Results,
    NoResults,
    Pending,
    Unavailable(String),
}

impl ActionBindingEditorState {
    pub(crate) fn has_unassigned_text(&self) -> bool {
        if self
            .selected_binding
            .as_ref()
            .is_some_and(|binding| self.active_binding.as_ref() != Some(binding))
        {
            return true;
        }
        let (assigned_query, assigned_mode, assigned_command, assigned_args) = match self
            .active_binding
            .as_ref()
        {
            Some(ActionBinding::LauncherQuery { query, mode }) => (query.as_str(), *mode, "", ""),
            Some(ActionBinding::ExactCommand { command, args }) => (
                "",
                QueryRunMode::OpenLauncher,
                command.as_str(),
                args.as_deref().unwrap_or_default(),
            ),
            _ => ("", QueryRunMode::OpenLauncher, "", ""),
        };
        let query_is_pinned_search = self
            .pinned_search_input
            .as_ref()
            .is_some_and(|(query, mode)| query == &self.query && *mode == self.query_mode);
        (!query_is_pinned_search
            && (self.query != assigned_query || self.query_mode != assigned_mode))
            || self.command != assigned_command
            || self.args != assigned_args
    }

    pub(crate) fn binding_for_explicit_apply(&self) -> Option<ActionBinding> {
        if !self.has_unassigned_text() {
            return None;
        }
        if self
            .selected_binding
            .as_ref()
            .is_some_and(|binding| self.active_binding.as_ref() != Some(binding))
            && self.selected_binding_input_digest == Some(self.authored_input_digest())
        {
            return self.selected_binding.clone();
        }
        match self.tab {
            EditorTab::Query if !self.query.trim().is_empty() => {
                Some(ActionBinding::LauncherQuery {
                    query: self.query.clone(),
                    mode: self.query_mode,
                })
            }
            EditorTab::Advanced if !self.command.trim().is_empty() => {
                Some(ActionBinding::ExactCommand {
                    command: self.command.clone(),
                    args: (!self.args.is_empty()).then(|| self.args.clone()),
                })
            }
            _ => None,
        }
    }

    pub(crate) fn discard_unassigned_changes(
        &mut self,
        scope: BindingEditorScope,
        binding: Option<&ActionBinding>,
    ) {
        self.reset_for(scope, binding);
    }

    #[cfg(test)]
    pub(crate) fn seed_pinned_search_for_test(
        &mut self,
        scope: BindingEditorScope,
        binding: ActionBinding,
        query: &str,
    ) {
        self.reset_for(scope, Some(&binding));
        self.query = query.into();
        self.accept_pinned_binding(binding);
    }

    #[cfg(test)]
    pub(crate) fn seed_search_result_for_test(
        &mut self,
        scope: BindingEditorScope,
        assigned: Option<&ActionBinding>,
        query: &str,
        candidate: ActionBinding,
    ) {
        self.reset_for(scope, assigned);
        self.query = query.into();
        self.search_rows = vec![
            crate::gui::universal_action_catalog::UniversalActionPickerRow::fixture(candidate),
        ];
        self.search_status = SearchStatus::Results;
    }

    #[cfg(test)]
    pub(crate) fn query_for_test(&self) -> &str {
        &self.query
    }

    #[cfg(test)]
    pub(crate) fn command_for_test(&self) -> &str {
        &self.command
    }

    #[cfg(test)]
    pub(crate) fn advanced_for_test(&self) -> bool {
        self.tab == EditorTab::Advanced
    }

    #[cfg(test)]
    pub(crate) fn seed_unassigned_query_for_test(
        &mut self,
        scope: BindingEditorScope,
        binding: Option<&ActionBinding>,
        query: &str,
    ) {
        self.reset_for(scope, binding);
        self.query = query.into();
    }

    fn reset_for(&mut self, scope: BindingEditorScope, binding: Option<&ActionBinding>) {
        self.trace_retiring_requests();
        *self = Self::from_binding(scope, binding);
    }

    fn from_binding(scope: BindingEditorScope, binding: Option<&ActionBinding>) -> Self {
        let mut state = Self {
            scope: Some(scope),
            active_binding: binding.cloned(),
            query_mode: QueryRunMode::OpenLauncher,
            editor_epoch: next_editor_epoch(),
            ..Self::default()
        };
        match binding {
            Some(ActionBinding::LauncherQuery { query, mode }) => {
                state.query.clone_from(query);
                state.query_mode = *mode;
            }
            Some(ActionBinding::ExactCommand { command, args }) => {
                state.tab = EditorTab::Advanced;
                state.command.clone_from(command);
                state.args = args.clone().unwrap_or_default();
            }
            Some(ActionBinding::Persisted { .. } | ActionBinding::Contextual { .. }) | None => {}
        }
        state
    }

    pub(crate) fn matches_scope(&self, scope: &BindingEditorScope) -> bool {
        self.scope.as_ref() == Some(scope)
    }

    fn matches_buffer_owner(&self, scope: &BindingEditorScope) -> bool {
        self.scope.as_ref().is_some_and(|current| {
            current.surface == scope.surface
                && current.editor_session == scope.editor_session
                && current.target == scope.target
                && current.slot == scope.slot
        })
    }

    fn rebind_draft_generation(&mut self, scope: BindingEditorScope) {
        self.trace_retiring_requests();
        let test_was_pending = self.pending_test_binding.is_some();
        self.editor_epoch = next_editor_epoch();
        self.query_request_generation = self.query_request_generation.wrapping_add(1).max(1);
        self.search_request_generation = self.search_request_generation.wrapping_add(1).max(1);
        self.test_request_generation = self.test_request_generation.wrapping_add(1).max(1);
        self.pending_search_due = None;
        self.pending_search_identity = None;
        self.pending_search_query = None;
        self.search_rows.clear();
        self.search_preview = None;
        self.search_preview_unavailable = None;
        self.pending_test_binding = None;
        self.pending_test_identity = None;
        self.pending_test_query = None;
        self.test_status =
            test_was_pending.then(|| "Pending Test was canceled because the draft changed".into());
        self.scope = Some(scope);
        if self.query.trim().is_empty() {
            self.search_status = SearchStatus::Idle;
        } else {
            self.search_status = SearchStatus::Pending;
            self.search_retry_attempts = 0;
            self.pending_search_due = Some(Instant::now() + SEARCH_DEBOUNCE);
        }
    }

    pub(crate) fn invalidate_visit(&mut self) {
        self.trace_retiring_requests();
        self.scope = None;
        self.pending_search_due = None;
        self.search_retry_attempts = 0;
        self.pending_test_binding = None;
        self.pending_test_identity = None;
        self.pending_test_query = None;
        self.pending_search_identity = None;
        self.pending_search_query = None;
    }

    /// End the buffer owner's lifetime after a completed close or force-close.
    /// A close prompt uses request invalidation instead, preserving its buffers
    /// until the user chooses whether to discard them.
    pub(crate) fn dispose(&mut self) {
        self.trace_retiring_requests();
        *self = Self::default();
    }

    /// Retire outstanding Search, Test, and confirmation identities without
    /// resetting the editor buffers. A close prompt may resolve with Keep
    /// Editing, in which case the same local draft should remain visible while
    /// late work from before the close attempt must stay invalid.
    pub(crate) fn invalidate_pending_requests(&mut self) {
        self.trace_retiring_requests();
        self.editor_epoch = next_editor_epoch();
        self.query_request_generation = self.query_request_generation.wrapping_add(1).max(1);
        self.search_request_generation = self.search_request_generation.wrapping_add(1).max(1);
        self.test_request_generation = self.test_request_generation.wrapping_add(1).max(1);
        self.pending_search_due = None;
        self.search_retry_attempts = 0;
        self.pending_search_identity = None;
        self.pending_search_query = None;
        self.pending_test_binding = None;
        self.pending_test_identity = None;
        self.pending_test_query = None;
        self.search_rows.clear();
        self.search_preview = None;
        self.search_preview_unavailable = None;
        self.search_status = SearchStatus::Idle;
        self.test_status = Some("Pending Test was canceled when closing the Designer".into());
    }

    fn trace_retiring_requests(&mut self) {
        if let (Some(identity), Some(query)) = (
            self.pending_search_identity.as_ref(),
            self.pending_search_query.as_deref(),
        ) {
            crate::radial::acceptance_trace::emit_authoring_provider_search(
                identity, "retired", "search", query, None, None,
            );
        }
        if let (Some(identity), Some(binding), Some(query)) = (
            self.pending_test_identity.as_ref(),
            self.pending_test_binding.as_ref(),
            self.pending_test_query.as_deref(),
        ) {
            crate::radial::acceptance_trace::emit_authoring_provider_search(
                identity,
                "retired",
                "test",
                query,
                Some(binding),
                None,
            );
        }
    }

    pub(crate) fn identity(&self) -> Option<AuthoringBindingEditorIdentity> {
        Some(AuthoringBindingEditorIdentity {
            scope: self.scope.clone()?,
            assigned_binding_digest: trace_binding_digest(self.active_binding.as_ref()),
            editor_epoch: self.editor_epoch,
            edit_generation: self.edit_generation,
            query_generation: self.query_generation,
            query_request_generation: self.query_request_generation,
            search_request_generation: self.search_request_generation,
            test_request_generation: self.test_request_generation,
        })
    }

    pub(crate) fn matches_identity(&self, identity: &AuthoringBindingEditorIdentity) -> bool {
        self.identity().as_ref() == Some(identity)
    }

    pub(crate) fn matches_search_identity(
        &self,
        identity: &AuthoringBindingEditorIdentity,
    ) -> bool {
        self.scope.as_ref() == Some(&identity.scope)
            && self.editor_epoch == identity.editor_epoch
            && self.query_generation == identity.query_generation
            && self.query_request_generation == identity.query_request_generation
            && self.search_request_generation == identity.search_request_generation
    }

    pub(crate) fn test_binding_matches(
        &self,
        identity: &AuthoringBindingEditorIdentity,
        binding: &ActionBinding,
    ) -> bool {
        self.matches_identity(identity) && self.pending_test_binding.as_ref() == Some(binding)
    }

    pub(crate) fn await_test_confirmation(
        &mut self,
        identity: &AuthoringBindingEditorIdentity,
        binding: &ActionBinding,
    ) -> bool {
        if !self.test_binding_matches(identity, binding) {
            return false;
        }
        self.pending_test_binding = None;
        self.pending_test_identity = None;
        self.pending_test_query = None;
        self.test_status = Some("Waiting for action confirmation".into());
        true
    }

    pub(crate) fn complete_search(&mut self, completion: ActionSearchCompletion) -> bool {
        if !self.matches_search_identity(&completion.identity) {
            return false;
        }
        self.pending_search_identity = None;
        self.pending_search_query = None;
        self.pending_search_due = None;
        self.search_rows = completion.rows;
        self.search_preview = completion.preview;
        self.search_preview_unavailable = completion.preview_unavailable;
        let retry_pending = matches!(&completion.state, ActionSearchCompletionState::Pending);
        self.search_status = match completion.state {
            ActionSearchCompletionState::Results => SearchStatus::Results,
            ActionSearchCompletionState::NoResults => SearchStatus::NoResults,
            ActionSearchCompletionState::Pending => SearchStatus::Pending,
            ActionSearchCompletionState::Unavailable(reason) => SearchStatus::Unavailable(reason),
        };
        if retry_pending {
            self.search_retry_attempts = self.search_retry_attempts.saturating_add(1);
            let factor = 1u32
                .checked_shl(u32::from(
                    self.search_retry_attempts.saturating_sub(1).min(4),
                ))
                .unwrap_or(u32::MAX);
            let delay = SEARCH_RETRY_BASE
                .saturating_mul(factor)
                .min(SEARCH_RETRY_MAX);
            self.pending_search_due = Some(Instant::now() + delay);
        } else {
            self.search_retry_attempts = 0;
        }
        true
    }

    pub(crate) fn finish_test(
        &mut self,
        identity: &AuthoringBindingEditorIdentity,
        binding: &ActionBinding,
        result: Result<(), String>,
    ) -> bool {
        if !self.matches_identity(identity)
            || self
                .pending_test_binding
                .as_ref()
                .is_some_and(|pending| pending != binding)
        {
            return false;
        }
        self.pending_test_binding = None;
        self.pending_test_identity = None;
        self.pending_test_query = None;
        self.test_status = Some(match result {
            Ok(()) => "Test action was sent to the normal action executor".into(),
            Err(reason) => format!("Test unavailable: {reason}"),
        });
        true
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        scope: BindingEditorScope,
        assigned_binding: Option<&ActionBinding>,
        invocation: &InvocationContext,
    ) -> Vec<ActionBindingEditorIntent> {
        self.show_with_action_catalog(ui, scope, assigned_binding, invocation, None)
    }

    pub(crate) fn show_with_action_catalog(
        &mut self,
        ui: &mut egui::Ui,
        scope: BindingEditorScope,
        assigned_binding: Option<&ActionBinding>,
        invocation: &InvocationContext,
        action_catalog: Option<
            &crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot,
        >,
    ) -> Vec<ActionBindingEditorIntent> {
        if !self.matches_buffer_owner(&scope) || self.active_binding.as_ref() != assigned_binding {
            self.reset_for(scope.clone(), assigned_binding);
        } else if !self.matches_scope(&scope) {
            self.rebind_draft_generation(scope.clone());
        }

        let mut intents = Vec::new();
        let now = Instant::now();
        let widget_scope = self.scope.clone().expect("scope initialized above");
        ui.push_id(widget_scope, |ui| {
            ui.horizontal(|ui| {
                let response = ui.selectable_label(self.tab == EditorTab::Query, "Query / Pin");
                if response.clicked() {
                    self.tab = EditorTab::Query;
                }
                trace_action_editor_control(
                    ui,
                    &response,
                    &self.identity().expect("editor scope initialized above"),
                    "query_tab",
                    None,
                    0,
                    0,
                    self.active_binding.as_ref(),
                    &self.query,
                    true,
                    self.tab == EditorTab::Query,
                );
                let response = ui.selectable_label(self.tab == EditorTab::Advanced, "Advanced");
                if response.clicked() {
                    self.tab = EditorTab::Advanced;
                }
                trace_action_editor_control(
                    ui,
                    &response,
                    &self.identity().expect("editor scope initialized above"),
                    "advanced_tab",
                    None,
                    0,
                    0,
                    self.active_binding.as_ref(),
                    &self.query,
                    true,
                    self.tab == EditorTab::Advanced,
                );
            });

            match self.tab {
                EditorTab::Query => self.query_ui(ui, invocation, now, &mut intents),
                EditorTab::Advanced => self.advanced_ui(ui, &mut intents),
            }

            if let Some(binding) = self.active_binding.clone() {
                let presentation = assigned_binding_presentation(&binding, action_catalog);
                self.assigned_binding_ui(ui, &binding, presentation, &mut intents);
            }
            if let Some(status) = &self.test_status {
                ui.small(status);
            }
        });

        if self.pending_search_due.is_some_and(|due| due <= now) {
            self.pending_search_due = None;
            self.emit_search(&mut intents);
        } else if let Some(due) = self.pending_search_due {
            ui.ctx()
                .request_repaint_after(due.saturating_duration_since(now));
        }
        intents
    }

    fn assigned_binding_ui(
        &mut self,
        ui: &mut egui::Ui,
        binding: &ActionBinding,
        presentation: (String, Option<String>),
        intents: &mut Vec<ActionBindingEditorIntent>,
    ) {
        ui.separator();
        let (summary, unavailable_reason) = presentation;
        ui.small(format!("Assigned: {summary}"));
        if let Some(reason) = &unavailable_reason {
            ui.colored_label(ui.visuals().warn_fg_color, format!("Unavailable: {reason}"));
        }
        let test_enabled = unavailable_reason.is_none();
        let response = ui.add_enabled(test_enabled, egui::Button::new("Test assigned binding"));
        if test_enabled && response.clicked() {
            self.emit_test(binding.clone(), intents);
        }
        trace_action_editor_control(
            ui,
            &response,
            &self.identity().expect("editor scope initialized above"),
            "test_assigned",
            None,
            0,
            0,
            Some(binding),
            &self.query,
            test_enabled,
            false,
        );
    }

    fn load_binding_fields(&mut self, binding: Option<&ActionBinding>) {
        self.trace_retiring_requests();
        self.query.clear();
        self.query_mode = QueryRunMode::OpenLauncher;
        self.command.clear();
        self.args.clear();
        self.tab = EditorTab::Query;
        self.search_rows.clear();
        self.search_preview = None;
        self.search_preview_unavailable = None;
        self.selected_binding = None;
        self.selected_binding_input_digest = None;
        self.pinned_search_input = None;
        self.search_status = SearchStatus::Idle;
        self.pending_search_due = None;
        self.search_retry_attempts = 0;
        self.pending_test_binding = None;
        self.pending_search_identity = None;
        self.pending_search_query = None;
        self.pending_test_identity = None;
        self.pending_test_query = None;
        match binding {
            Some(ActionBinding::LauncherQuery { query, mode }) => {
                self.query.clone_from(query);
                self.query_mode = *mode;
            }
            Some(ActionBinding::ExactCommand { command, args }) => {
                self.tab = EditorTab::Advanced;
                self.command.clone_from(command);
                self.args = args.clone().unwrap_or_default();
            }
            Some(ActionBinding::Persisted { .. } | ActionBinding::Contextual { .. }) | None => {}
        }
        self.edit_generation = self.edit_generation.wrapping_add(1).max(1);
        self.query_generation = self.query_generation.wrapping_add(1).max(1);
        self.query_request_generation = self.query_request_generation.wrapping_add(1).max(1);
        self.search_request_generation = self.search_request_generation.wrapping_add(1).max(1);
    }

    fn query_ui(
        &mut self,
        ui: &mut egui::Ui,
        invocation: &InvocationContext,
        now: Instant,
        intents: &mut Vec<ActionBindingEditorIntent>,
    ) {
        // Earlier horizontal controls can expand the parent min rect; keep the pane's entry width.
        let column_width_at_query_start = ui.available_width();
        ui.label("Search the launcher, then pin an action or save the live query.");
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Search launcher results")
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            self.query_request_generation = self.query_request_generation.wrapping_add(1).max(1);
            self.query_generation = self.query_generation.wrapping_add(1).max(1);
            self.bump_edit_generation();
            self.search_rows.clear();
            self.search_preview = None;
            self.search_preview_unavailable = None;
            self.clear_selected_binding();
            self.search_status = SearchStatus::Pending;
            self.search_retry_attempts = 0;
            self.pending_search_due = Some(now + SEARCH_DEBOUNCE);
            self.pending_test_binding = None;
        }
        let query_input_binding = ActionBinding::LauncherQuery {
            query: self.query.clone(),
            mode: self.query_mode,
        };
        let enter_pressed =
            response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        trace_action_editor_control_with_presentation(
            ui,
            &response,
            &self.identity().expect("editor scope initialized above"),
            "query_field",
            None,
            0,
            [0; 3],
            0,
            Some(&query_input_binding),
            &self.query,
            true,
            false,
            response.changed(),
            enter_pressed,
        );
        if enter_pressed {
            self.pending_search_due = None;
            self.search_retry_attempts = 0;
            self.emit_search(intents);
        }
        let response = ui.button("Search / preview");
        if response.clicked() {
            self.pending_search_due = None;
            self.search_retry_attempts = 0;
            self.emit_search(intents);
        }
        let search_binding = ActionBinding::LauncherQuery {
            query: self.query.clone(),
            mode: self.query_mode,
        };
        trace_action_editor_control(
            ui,
            &response,
            &self.identity().expect("editor scope initialized above"),
            "search",
            None,
            0,
            0,
            Some(&search_binding),
            &self.query,
            true,
            false,
        );

        ui.horizontal(|ui| {
            ui.label("Saved query mode");
            let mut execute_first = self.query_mode == QueryRunMode::ExecuteFirst;
            let response = ui.checkbox(&mut execute_first, "Auto Submit (execute first result)");
            if response.changed() {
                self.query_mode = if execute_first {
                    QueryRunMode::ExecuteFirst
                } else {
                    QueryRunMode::OpenLauncher
                };
                self.clear_selected_binding();
                self.bump_edit_generation();
            }
            let mode_binding = ActionBinding::LauncherQuery {
                query: self.query.clone(),
                mode: self.query_mode,
            };
            trace_action_editor_control(
                ui,
                &response,
                &self.identity().expect("editor scope initialized above"),
                "query_mode",
                None,
                0,
                0,
                Some(&mode_binding),
                &self.query,
                true,
                self.query_mode == QueryRunMode::ExecuteFirst,
            );
        });
        match &self.search_status {
            SearchStatus::Idle => {
                ui.small("Search to preview the current launcher results.");
            }
            SearchStatus::Pending => {
                ui.small("Search pending or provider results are still changing.");
            }
            SearchStatus::Results => {
                if self.query_mode == QueryRunMode::ExecuteFirst {
                    if let Some(preview) = &self.search_preview {
                        ui.label(format!("Would execute: {preview}"));
                    } else if let Some(reason) = &self.search_preview_unavailable {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!(
                                "First result unavailable: {reason}. Later results will not be substituted."
                            ),
                        );
                    } else {
                        ui.label("The first result has no available action; later results will not be substituted.");
                    }
                } else {
                    ui.label(format!("Would open the launcher with {:?}", self.query));
                    if let Some(preview) = &self.search_preview {
                        ui.small(format!("Current first result: {preview}"));
                    } else if let Some(reason) = &self.search_preview_unavailable {
                        ui.small(format!(
                            "Current first result action is unavailable: {reason}"
                        ));
                    }
                }
            }
            SearchStatus::NoResults => {
                ui.label("No launcher results. Saving this query still opens it for interaction.");
            }
            SearchStatus::Unavailable(reason) => {
                ui.colored_label(ui.visuals().warn_fg_color, reason);
            }
        };

        ui.horizontal(|ui| {
            let save_enabled = !self.query.trim().is_empty();
            let binding = ActionBinding::LauncherQuery {
                query: self.query.clone(),
                mode: self.query_mode,
            };
            let response = ui.add_enabled(save_enabled, egui::Button::new("Save query"));
            if response.clicked() {
                intents.push(ActionBindingEditorIntent::SaveQuery {
                    binding: binding.clone(),
                });
            }
            trace_action_editor_control(
                ui,
                &response,
                &self.identity().expect("editor scope initialized above"),
                "save_query",
                None,
                0,
                0,
                Some(&binding),
                &self.query,
                save_enabled,
                false,
            );
            let response = ui.add_enabled(save_enabled, egui::Button::new("Test query"));
            if response.clicked() {
                self.emit_test(binding.clone(), intents);
            }
            trace_action_editor_control(
                ui,
                &response,
                &self.identity().expect("editor scope initialized above"),
                "test_query",
                None,
                0,
                0,
                Some(&binding),
                &self.query,
                save_enabled,
                false,
            );
        });

        let row_count = self.search_rows.len();
        let result_count_response = ui.small(format!("{row_count} target/action result(s)"));
        trace_action_editor_control(
            ui,
            &result_count_response,
            &self.identity().expect("editor scope initialized above"),
            "result_count",
            // The native trace uses the summary's numeric control index as its
            // bounded result count so the acceptance runner can bind row orders
            // to the actual filtered catalog size without recording result text.
            Some(row_count),
            0,
            0,
            None,
            &self.query,
            true,
            false,
        );
        let scroll_trace_identity = self.identity().expect("editor scope initialized above");
        let reserve_parent_scrollbar = self
            .scope
            .as_ref()
            .is_some_and(|scope| scope.surface == BindingEditorSurface::Inspector);
        let result_scroll = in_result_scroll_column(
            ui,
            reserve_parent_scrollbar,
            column_width_at_query_start,
            |ui| {
                egui::ScrollArea::vertical()
                    .max_height(RESULT_SCROLL_MAX_HEIGHT)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        let rows = self.search_rows.clone();
                        for (row_index, row) in rows.iter().enumerate() {
                            let selected = row.binding.as_ref() == self.selected_binding.as_ref();
                            let row_identity = trace_picker_row_identity(row);
                            // Pinning can advance this editor's edit generation in the
                            // same egui frame. Keep the displayed row and the button
                            // response correlated to the search identity that rendered
                            // them; the next frame publishes the new editor identity.
                            let rendered_editor_identity =
                                self.identity().expect("editor scope initialized above");
                            let display_label = row.display_label();
                            ui.vertical(|ui| {
                                // Keep the complete target/action identity inside the
                                // Inspector's narrow column. The action controls live
                                // on a second line so they cannot steal the width the
                                // label needs to wrap.
                                let response = ui
                                    .add(
                                        egui::Button::new(&display_label)
                                            .wrap(true)
                                            .selected(selected),
                                    )
                                    .on_hover_text(
                                        row.unavailable_reason
                                            .as_deref()
                                            .or(row.availability.disabled_reason())
                                            .unwrap_or("Target and action identity"),
                                    );
                                if response.clicked() {
                                    self.select_binding_candidate(row.binding.clone());
                                }
                                trace_action_editor_control_with_full_text(
                                    ui,
                                    &response,
                                    &rendered_editor_identity,
                                    "result_target",
                                    Some(row_index),
                                    row_identity.target,
                                    [
                                        row_identity.title,
                                        row_identity.target_type,
                                        row_identity.disambiguator,
                                    ],
                                    row_identity.action,
                                    row.binding.as_ref(),
                                    &self.query,
                                    true,
                                    self.selected_binding.as_ref() == row.binding.as_ref(),
                                    response.changed(),
                                    false,
                                    Some(&display_label),
                                );
                                let assignable = row.binding.is_some();
                                let reason = row
                                    .unavailable_reason
                                    .as_deref()
                                    .or(row.availability.disabled_reason())
                                    .unwrap_or("This live target cannot be saved");
                                if !assignable || !row.availability.is_available() {
                                    ui.small(format!("Availability: unavailable — {reason}"));
                                }
                                ui.horizontal(|ui| {
                                    let response = ui.add_enabled(
                                        assignable,
                                        egui::Button::new("Pin this action"),
                                    );
                                    if response.clicked()
                                        && let Ok(binding) = row.assignment()
                                    {
                                        self.stage_pinned_binding(binding.clone());
                                        intents.push(ActionBindingEditorIntent::Pin { binding });
                                    }
                                    trace_action_editor_control_with_presentation(
                                        ui,
                                        &response,
                                        &rendered_editor_identity,
                                        "pin_result",
                                        Some(row_index),
                                        row_identity.target,
                                        [
                                            row_identity.title,
                                            row_identity.target_type,
                                            row_identity.disambiguator,
                                        ],
                                        row_identity.action,
                                        row.binding.as_ref(),
                                        &self.query,
                                        assignable,
                                        self.selected_binding.as_ref() == row.binding.as_ref(),
                                        response.changed(),
                                        false,
                                    );
                                    let test_enabled =
                                        assignable && row.availability.is_available();
                                    let response =
                                        ui.add_enabled(test_enabled, egui::Button::new("Test"));
                                    if response.clicked()
                                        && let Ok(binding) = row.assignment()
                                    {
                                        self.emit_test(binding, intents);
                                    }
                                    trace_action_editor_control_with_presentation(
                                        ui,
                                        &response,
                                        &self.identity().expect("editor scope initialized above"),
                                        "test_result",
                                        Some(row_index),
                                        row_identity.target,
                                        [
                                            row_identity.title,
                                            row_identity.target_type,
                                            row_identity.disambiguator,
                                        ],
                                        row_identity.action,
                                        row.binding.as_ref(),
                                        &self.query,
                                        test_enabled,
                                        self.selected_binding.as_ref() == row.binding.as_ref(),
                                        response.changed(),
                                        false,
                                    );
                                });
                            });
                        }
                    })
            },
        );
        if self.identity().as_ref() == Some(&scroll_trace_identity) {
            trace_action_editor_scroll(ui, &result_scroll, &scroll_trace_identity, &self.query);
        }

        egui::CollapsingHeader::new("Contextual window actions")
            .id_source(("contextual-action-browse", self.scope.as_ref()))
            .show(ui, |ui| {
                if self.contextual_cache.as_ref().is_none_or(|(cached, _)| cached != invocation) {
                    let rows = crate::gui::universal_action_catalog::UniversalActionAuthoringCatalog::contextual_window_rows(invocation);
                    self.contextual_cache = Some((invocation.clone(), rows));
                }
                let rows = self.contextual_cache.as_ref().map(|(_, rows)| rows.clone()).unwrap_or_default();
                for (row_index, row) in rows.iter().enumerate() {
                    let row_identity = trace_picker_row_identity(row);
                    let display_label = row.display_label();
                    ui.vertical(|ui| {
                        let response = ui.add(
                            egui::Button::new(&display_label)
                                .wrap(true)
                                .selected(self.selected_binding.as_ref() == row.binding.as_ref()),
                        );
                        if response.clicked() {
                            self.select_binding_candidate(row.binding.clone());
                        }
                        trace_action_editor_control_with_full_text(
                            ui,
                            &response,
                            &self.identity().expect("editor scope initialized above"),
                            "contextual_target",
                            Some(row_index),
                            row_identity.target,
                            [
                                row_identity.title,
                                row_identity.target_type,
                                row_identity.disambiguator,
                            ],
                            row_identity.action,
                            row.binding.as_ref(),
                            &self.query,
                            true,
                            self.selected_binding.as_ref() == row.binding.as_ref(),
                            response.changed(),
                            false,
                            Some(&display_label),
                        );
                        if !row.availability.is_available() {
                            ui.small(format!(
                                "Availability: unavailable — {}",
                                row.unavailable_reason
                                    .as_deref()
                                    .or(row.availability.disabled_reason())
                                    .unwrap_or("Contextual action is unavailable")
                            ));
                        }
                        let pin_enabled = row.availability.is_available();
                        ui.horizontal(|ui| {
                            let response = ui.add_enabled(
                                pin_enabled,
                                egui::Button::new("Pin contextual action"),
                            );
                            if response.clicked() && let Ok(binding) = row.assignment() {
                                self.stage_pinned_binding(binding.clone());
                                intents.push(ActionBindingEditorIntent::Pin { binding });
                            }
                            trace_action_editor_control_with_presentation(
                                ui,
                                &response,
                                &self.identity().expect("editor scope initialized above"),
                                "pin_contextual",
                                Some(row_index),
                                row_identity.target,
                                [
                                    row_identity.title,
                                    row_identity.target_type,
                                    row_identity.disambiguator,
                                ],
                                row_identity.action,
                                row.binding.as_ref(),
                                &self.query,
                                pin_enabled,
                                self.selected_binding.as_ref() == row.binding.as_ref(),
                                response.changed(),
                                false,
                            );
                        });
                    });
                }
            });
    }

    fn advanced_ui(&mut self, ui: &mut egui::Ui, intents: &mut Vec<ActionBindingEditorIntent>) {
        ui.label("Exact command uses the launcher's existing parser and arguments.");
        let command_response = ui.add(
            egui::TextEdit::singleline(&mut self.command)
                .hint_text("Exact command")
                .desired_width(f32::INFINITY),
        );
        let args_response = ui.add(
            egui::TextEdit::singleline(&mut self.args)
                .hint_text("Arguments (optional)")
                .desired_width(f32::INFINITY),
        );
        if command_response.changed() || args_response.changed() {
            self.clear_selected_binding();
            self.bump_edit_generation();
        }
        let current_exact_binding =
            (!self.command.trim().is_empty()).then(|| ActionBinding::ExactCommand {
                command: self.command.clone(),
                args: (!self.args.is_empty()).then(|| self.args.clone()),
            });
        trace_action_editor_control(
            ui,
            &command_response,
            &self.identity().expect("editor scope initialized above"),
            "exact_command_field",
            None,
            0,
            0,
            current_exact_binding.as_ref(),
            &self.query,
            true,
            false,
        );
        trace_action_editor_control(
            ui,
            &args_response,
            &self.identity().expect("editor scope initialized above"),
            "exact_args_field",
            None,
            0,
            0,
            current_exact_binding.as_ref(),
            &self.query,
            true,
            false,
        );
        if !self.command.trim().is_empty() {
            let binding = ActionBinding::ExactCommand {
                command: self.command.clone(),
                args: (!self.args.is_empty()).then(|| self.args.clone()),
            };
            match crate::radial::bindings::prepare_deferred_binding(&binding) {
                Some(crate::radial::bindings::DeferredBindingPreparation {
                    binding: crate::radial::dynamic::FrozenBinding::Deferred { kind, .. },
                    ..
                }) => ui.small(kind.reason()),
                _ => ui.small("Exact command is not valid."),
            };
            ui.horizontal(|ui| {
                let response = ui.button("Use exact command");
                if response.clicked() {
                    intents.push(ActionBindingEditorIntent::SetCommand {
                        binding: binding.clone(),
                    });
                }
                trace_action_editor_control(
                    ui,
                    &response,
                    &self.identity().expect("editor scope initialized above"),
                    "use_exact_command",
                    None,
                    0,
                    0,
                    Some(&binding),
                    &self.query,
                    true,
                    false,
                );
                let valid = crate::radial::bindings::prepare_deferred_binding(&binding)
                    .is_some_and(|prepared| {
                        !matches!(
                            prepared.binding,
                            crate::radial::dynamic::FrozenBinding::Deferred {
                                kind: crate::radial::dynamic::DeferredBindingKind::ExactCommand {
                                    disposition:
                                        crate::radial::dynamic::ExactCommandDisposition::Invalid,
                                    ..
                                },
                                ..
                            }
                        )
                    });
                let response = ui.add_enabled(valid, egui::Button::new("Test exact command"));
                if response.clicked() {
                    self.emit_test(binding.clone(), intents);
                }
                trace_action_editor_control(
                    ui,
                    &response,
                    &self.identity().expect("editor scope initialized above"),
                    "test_exact_command",
                    None,
                    0,
                    0,
                    Some(&binding),
                    &self.query,
                    valid,
                    false,
                );
            });
        } else {
            ui.small("Enter a command to test or assign it.");
        }
    }

    fn emit_search(&mut self, intents: &mut Vec<ActionBindingEditorIntent>) {
        self.search_request_generation = self.search_request_generation.wrapping_add(1).max(1);
        let Some(identity) = self.identity() else {
            return;
        };
        self.search_status = SearchStatus::Pending;
        self.pending_search_identity = Some(identity.clone());
        self.pending_search_query = Some(self.query.clone());
        intents.push(ActionBindingEditorIntent::Search {
            identity,
            query: self.query.clone(),
        });
    }

    fn emit_test(&mut self, binding: ActionBinding, intents: &mut Vec<ActionBindingEditorIntent>) {
        let Some(mut identity) = self.identity() else {
            return;
        };
        self.test_request_generation = self.test_request_generation.wrapping_add(1).max(1);
        identity.test_request_generation = self.test_request_generation;
        self.pending_test_binding = Some(binding.clone());
        self.pending_test_identity = Some(identity.clone());
        self.pending_test_query = Some(match &binding {
            ActionBinding::LauncherQuery { query, .. } => query.clone(),
            ActionBinding::Persisted { .. }
            | ActionBinding::Contextual { .. }
            | ActionBinding::ExactCommand { .. } => self.query.clone(),
        });
        self.test_status = Some("Test pending".into());
        intents.push(ActionBindingEditorIntent::Test {
            identity,
            binding,
            history_query: self.query.clone(),
        });
    }

    /// Finalize the editor state only after its owning draft accepted the
    /// assignment. A rejected model mutation leaves the authored buffers and
    /// staged Pin candidate available for correction or retry.
    pub(crate) fn finish_assignment<E>(
        &mut self,
        binding: ActionBinding,
        kind: BindingAssignmentKind,
        result: Result<(), E>,
    ) -> Result<(), E> {
        result?;
        self.accept_committed_assignment(binding, kind);
        Ok(())
    }

    pub(crate) fn accept_committed_assignment(
        &mut self,
        binding: ActionBinding,
        kind: BindingAssignmentKind,
    ) {
        match kind {
            BindingAssignmentKind::Explicit => self.accept_binding(binding),
            BindingAssignmentKind::Pinned => self.accept_pinned_binding(binding),
        }
    }

    fn accept_binding(&mut self, binding: ActionBinding) {
        self.accept_binding_without_generation(binding);
        self.bump_edit_generation();
    }

    fn accept_pinned_binding(&mut self, binding: ActionBinding) {
        let pinned_search = matches!(
            &binding,
            ActionBinding::Persisted { .. } | ActionBinding::Contextual { .. }
        )
        .then(|| (self.query.clone(), self.query_mode));
        self.accept_binding_without_generation(binding.clone());
        self.bump_edit_generation();
        // Pin is the one assignment path that intentionally keeps the result
        // highlighted while preserving its search text as browsing state.
        if let Some((query, mode)) = pinned_search {
            self.query = query.clone();
            self.query_mode = mode;
            self.pinned_search_input = Some((query, mode));
        }
        self.selected_binding = Some(binding.clone());
        self.selected_binding_input_digest = Some(self.authored_input_digest());
    }

    fn accept_binding_without_generation(&mut self, binding: ActionBinding) {
        // A Save query / Set command decision supersedes any highlighted live
        // result. Leaving that candidate selected makes the next explicit
        // Apply prefer the stale row over the binding the user just saved.
        self.clear_selected_binding();
        match &binding {
            ActionBinding::LauncherQuery { query, mode } => {
                self.query.clone_from(query);
                self.query_mode = *mode;
                self.command.clear();
                self.args.clear();
            }
            ActionBinding::ExactCommand { command, args } => {
                self.query.clear();
                self.query_mode = QueryRunMode::OpenLauncher;
                self.command.clone_from(command);
                self.args = args.clone().unwrap_or_default();
            }
            ActionBinding::Persisted { .. } | ActionBinding::Contextual { .. } => {
                self.query.clear();
                self.query_mode = QueryRunMode::OpenLauncher;
                self.command.clear();
                self.args.clear();
            }
        }
        self.active_binding = Some(binding);
        self.pinned_search_input = None;
    }

    fn stage_pinned_binding(&mut self, binding: ActionBinding) {
        self.selected_binding_input_digest = Some(self.authored_input_digest());
        self.selected_binding = Some(binding);
    }

    fn select_binding_candidate(&mut self, binding: Option<ActionBinding>) {
        self.selected_binding = binding;
        self.selected_binding_input_digest = self
            .selected_binding
            .as_ref()
            .map(|_| self.authored_input_digest());
        self.bump_edit_generation();
    }

    fn clear_selected_binding(&mut self) {
        self.selected_binding = None;
        self.selected_binding_input_digest = None;
    }

    pub(crate) fn set_assigned_binding(&mut self, binding: Option<ActionBinding>) {
        if self.active_binding != binding {
            self.active_binding = binding.clone();
            self.load_binding_fields(binding.as_ref());
        }
    }

    pub(crate) fn selected_binding(&self) -> Option<ActionBinding> {
        self.selected_binding.clone()
    }

    pub(crate) fn authored_input_digest(&self) -> u64 {
        let mode = match self.query_mode {
            QueryRunMode::OpenLauncher => "open_launcher",
            QueryRunMode::ExecuteFirst => "execute_first",
        };
        trace_private_digest(&[&self.query, mode, &self.command, &self.args])
    }

    pub(crate) fn acceptance_observation(
        &self,
    ) -> Option<crate::gui::query_observation::ActionEditorObservation> {
        let identity = self.identity()?;
        let surface = match identity.scope.surface {
            BindingEditorSurface::Properties => "properties",
            BindingEditorSurface::Inspector => "inspector",
        };
        let mut target_hasher = std::collections::hash_map::DefaultHasher::new();
        identity.scope.target.hash(&mut target_hasher);
        let mut results_digest = 0xcbf29ce484222325u64;
        for row in &self.search_rows {
            let identity = trace_picker_row_identity(row);
            for digest in [
                identity.target,
                identity.title,
                identity.target_type,
                identity.disambiguator,
                identity.action,
                identity.binding,
            ] {
                results_digest = trace_private_digest_bytes(results_digest, &digest.to_le_bytes());
            }
        }
        Some(crate::gui::query_observation::ActionEditorObservation {
            surface: surface.into(),
            editor_session_id: identity.scope.editor_session.0,
            draft_generation: identity.scope.draft_generation.0,
            stable_target_digest: target_hasher.finish(),
            editor_epoch: identity.editor_epoch,
            edit_generation: identity.edit_generation,
            query_generation: identity.query_generation,
            query_request_generation: identity.query_request_generation,
            search_request_generation: identity.search_request_generation,
            test_request_generation: identity.test_request_generation,
            query_digest: crate::radial::acceptance_trace::private_trace_text_digest(&self.query),
            authored_input_digest: self.authored_input_digest(),
            assigned_binding_digest: trace_binding_digest(self.active_binding.as_ref()),
            selected_binding_digest: trace_binding_digest(self.selected_binding.as_ref()),
            search_pending: self.search_status == SearchStatus::Pending,
            test_pending: self.pending_test_binding.is_some(),
            result_count: self.search_rows.len(),
            results_digest,
        })
    }

    fn bump_edit_generation(&mut self) {
        self.edit_generation = self.edit_generation.wrapping_add(1).max(1);
        if let (Some(identity), Some(binding), Some(query)) = (
            self.pending_test_identity.as_ref(),
            self.pending_test_binding.as_ref(),
            self.pending_test_query.as_deref(),
        ) {
            crate::radial::acceptance_trace::emit_authoring_provider_search(
                identity,
                "retired",
                "test",
                query,
                Some(binding),
                None,
            );
        }
        self.pending_test_binding = None;
        self.pending_test_identity = None;
        self.pending_test_query = None;
        self.test_status = None;
    }
}

fn binding_summary(binding: &ActionBinding) -> String {
    match binding {
        ActionBinding::Persisted { action } => format!(
            "Pinned action {} ({})",
            action.action_id,
            action
                .target
                .as_ref()
                .map_or("global".into(), |target| format!("{target:?}"))
        ),
        ActionBinding::Contextual {
            selector,
            action_id,
        } => {
            format!("Contextual action {action_id} for {selector:?}")
        }
        ActionBinding::LauncherQuery { query, mode } => format!(
            "Saved query {query:?} · {}",
            match mode {
                QueryRunMode::OpenLauncher => "opens launcher",
                QueryRunMode::ExecuteFirst => "executes first result",
            }
        ),
        ActionBinding::ExactCommand { command, args } => {
            format!("Exact command {command:?} · args {:?}", args)
        }
    }
}

fn assigned_binding_presentation(
    binding: &ActionBinding,
    action_catalog: Option<&crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot>,
) -> (String, Option<String>) {
    let ActionBinding::Persisted { action } = binding else {
        return (binding_summary(binding), None);
    };
    let fallback_summary = binding_summary(binding);
    let Some(action_catalog) = action_catalog else {
        return (
            fallback_summary,
            Some("the current action catalog is unavailable".into()),
        );
    };
    let resolved = action_catalog
        .resolve_persisted_action(
            action,
            crate::universal_actions::ActionSurface::RadialMenu,
            "",
        )
        .map_err(|error| error.to_string());
    assigned_persisted_action_presentation(binding, resolved)
}

fn assigned_persisted_action_presentation(
    binding: &ActionBinding,
    resolved: Result<crate::universal_actions::ResolvedPersistedAction, String>,
) -> (String, Option<String>) {
    match resolved {
        Err(error) => (binding_summary(binding), Some(error)),
        Ok(resolved) => {
            let summary = crate::gui::universal_action_catalog::resolved_action_display_label(
                &resolved.target,
                &resolved.action,
                crate::universal_actions::ActionSurface::RadialMenu,
            );
            (
                summary,
                resolved
                    .action
                    .availability
                    .disabled_reason()
                    .map(str::to_owned),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::radial_editor::{PropertiesDraft, RadialEditorState};

    #[test]
    fn action_editor_and_request_query_digests_share_the_same_recipe() {
        let query = "fixture query with private wording";
        assert_eq!(
            trace_private_digest(&[query]),
            crate::radial::acceptance_trace::private_trace_text_digest(query),
        );
    }

    #[test]
    fn acceptance_snapshot_contains_only_editor_identity_digests_and_state() {
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope(), None);
        state.query = "private query value".into();
        let observation = state.acceptance_observation().expect("active editor scope");
        assert_eq!(observation.surface, "properties");
        assert_eq!(observation.editor_session_id, 7);
        assert_eq!(
            observation.query_digest,
            trace_private_digest(&["private query value"])
        );
        let serialized = serde_json::to_string(&observation).unwrap();
        assert!(!serialized.contains("private query value"));
    }

    #[test]
    fn authored_input_digest_tracks_exact_command_arguments_and_mode_without_counters() {
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope(), None);
        state.query = "saved query".into();
        state.command = "window:close".into();
        state.args = "first argument".into();
        let baseline = state.authored_input_digest();

        state.command = "window:show".into();
        assert_ne!(state.authored_input_digest(), baseline);
        state.command = "window:close".into();
        state.args = "second argument".into();
        assert_ne!(state.authored_input_digest(), baseline);
        state.args = "first argument".into();
        state.query_mode = QueryRunMode::ExecuteFirst;
        assert_ne!(state.authored_input_digest(), baseline);

        let digest_before_counter_change = state.authored_input_digest();
        let identity = state.identity().expect("same editor identity");
        state.query_request_generation = state.query_request_generation.wrapping_add(1);
        assert_ne!(
            state.identity().unwrap().query_request_generation,
            identity.query_request_generation
        );
        assert_eq!(state.authored_input_digest(), digest_before_counter_change);
    }

    #[test]
    fn explicit_apply_captures_query_and_exact_buffers_and_discard_restores_assignment() {
        let original = ActionBinding::LauncherQuery {
            query: "assigned query".into(),
            mode: QueryRunMode::OpenLauncher,
        };
        let mut state = ActionBindingEditorState::default();
        let mut inspector_scope = scope();
        inspector_scope.surface = BindingEditorSurface::Inspector;
        state.reset_for(inspector_scope.clone(), Some(&original));

        state.query = "new query".into();
        state.query_mode = QueryRunMode::ExecuteFirst;
        assert_eq!(
            state.binding_for_explicit_apply(),
            Some(ActionBinding::LauncherQuery {
                query: "new query".into(),
                mode: QueryRunMode::ExecuteFirst,
            })
        );

        state.tab = EditorTab::Advanced;
        state.command = "window:close".into();
        state.args = "--all".into();
        assert_eq!(
            state.binding_for_explicit_apply(),
            Some(ActionBinding::ExactCommand {
                command: "window:close".into(),
                args: Some("--all".into()),
            })
        );

        state.discard_unassigned_changes(inspector_scope.clone(), Some(&original));
        assert!(!state.has_unassigned_text());
        assert_eq!(state.query, "assigned query");
        assert_eq!(state.active_binding, Some(original));
    }

    #[test]
    fn explicit_apply_uses_authored_text_changed_after_selecting_a_result() {
        let mut inspector_scope = scope();
        inspector_scope.surface = BindingEditorSurface::Inspector;
        let original = ActionBinding::LauncherQuery {
            query: "assigned query".into(),
            mode: QueryRunMode::OpenLauncher,
        };
        let result = ActionBinding::Persisted {
            action: crate::universal_actions::PersistedUniversalActionRef {
                target: None,
                action_id: crate::universal_actions::ActionId::new("help.show"),
            },
        };
        let mut state = ActionBindingEditorState::default();
        state.reset_for(inspector_scope.clone(), Some(&original));
        state.select_binding_candidate(Some(result.clone()));
        assert_eq!(state.binding_for_explicit_apply(), Some(result));

        let saved_query = ActionBinding::LauncherQuery {
            query: "query saved after selecting a result".into(),
            mode: QueryRunMode::OpenLauncher,
        };
        state.query = "query saved after selecting a result".into();
        state.accept_binding(saved_query.clone());
        assert_eq!(state.selected_binding(), None);
        assert!(!state.has_unassigned_text());
        assert_eq!(state.binding_for_explicit_apply(), None);
        assert_eq!(state.active_binding, Some(saved_query));

        state.tab = EditorTab::Advanced;
        state.command = "window:close".into();
        state.args = "--all".into();
        assert_eq!(
            state.binding_for_explicit_apply(),
            Some(ActionBinding::ExactCommand {
                command: "window:close".into(),
                args: Some("--all".into()),
            })
        );

        state.reset_for(inspector_scope, Some(&original));
        state.select_binding_candidate(Some(ActionBinding::ExactCommand {
            command: "window:show".into(),
            args: None,
        }));
        state.query = "new query text".into();
        state.query_mode = QueryRunMode::ExecuteFirst;
        assert_eq!(
            state.binding_for_explicit_apply(),
            Some(ActionBinding::LauncherQuery {
                query: "new query text".into(),
                mode: QueryRunMode::ExecuteFirst,
            })
        );
    }

    #[test]
    fn pinned_search_text_is_browsing_until_the_user_edits_it() {
        let mut inspector_scope = scope();
        inspector_scope.surface = BindingEditorSurface::Inspector;
        let binding = persisted_binding(
            crate::universal_actions::PersistableActionTargetRef::Note {
                slug: "pinned-search-fixture".into(),
            },
            crate::universal_actions::action_ids::NOTE_OPEN,
        );
        let mut state = ActionBindingEditorState::default();
        state.reset_for(inspector_scope, None);
        state.query = "find the action to pin".into();
        state.accept_pinned_binding(binding.clone());

        assert_eq!(state.query, "find the action to pin");
        assert_eq!(state.active_binding, Some(binding));
        assert!(!state.has_unassigned_text());

        state.query = "a newly authored query".into();
        assert!(state.has_unassigned_text());
        assert_eq!(
            state.binding_for_explicit_apply(),
            Some(ActionBinding::LauncherQuery {
                query: "a newly authored query".into(),
                mode: QueryRunMode::OpenLauncher,
            })
        );
    }

    #[test]
    fn explicit_assignments_clear_inactive_tab_text_and_later_edits_remain_dirty() {
        let original_query = ActionBinding::LauncherQuery {
            query: "previously assigned query".into(),
            mode: QueryRunMode::OpenLauncher,
        };
        let mut inspector_scope = scope();
        inspector_scope.surface = BindingEditorSurface::Inspector;

        let mut query_state = ActionBindingEditorState::default();
        query_state.reset_for(inspector_scope.clone(), Some(&original_query));
        query_state.tab = EditorTab::Advanced;
        query_state.command = "stale exact command".into();
        query_state.args = "stale arguments".into();
        let saved_query = ActionBinding::LauncherQuery {
            query: "newly saved query".into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        query_state
            .accept_committed_assignment(saved_query.clone(), BindingAssignmentKind::Explicit);

        assert_eq!(query_state.active_binding, Some(saved_query.clone()));
        assert_eq!(query_state.query, "newly saved query");
        assert_eq!(query_state.query_mode, QueryRunMode::ExecuteFirst);
        assert!(query_state.command.is_empty());
        assert!(query_state.args.is_empty());
        assert!(!query_state.has_unassigned_text());

        query_state.command = "a later exact command edit".into();
        assert!(query_state.has_unassigned_text());

        let mut exact_state = ActionBindingEditorState::default();
        exact_state.reset_for(inspector_scope, Some(&original_query));
        exact_state.query = "stale query tab text".into();
        exact_state.query_mode = QueryRunMode::ExecuteFirst;
        exact_state.command = "window:show".into();
        exact_state.args = "--all".into();
        let exact = ActionBinding::ExactCommand {
            command: "window:close".into(),
            args: Some("--all".into()),
        };
        exact_state.accept_committed_assignment(exact.clone(), BindingAssignmentKind::Explicit);

        assert_eq!(exact_state.active_binding, Some(exact.clone()));
        assert!(exact_state.query.is_empty());
        assert_eq!(exact_state.query_mode, QueryRunMode::OpenLauncher);
        assert_eq!(exact_state.command, "window:close");
        assert_eq!(exact_state.args, "--all");
        assert!(!exact_state.has_unassigned_text());

        exact_state.query = "a later query edit".into();
        assert!(exact_state.has_unassigned_text());
    }

    #[test]
    fn pin_after_advanced_edit_consumes_command_but_keeps_search_as_browsing_text() {
        let original = ActionBinding::ExactCommand {
            command: "window:show".into(),
            args: None,
        };
        let pinned = persisted_binding(
            crate::universal_actions::PersistableActionTargetRef::Note {
                slug: "pin-after-advanced-fixture".into(),
            },
            crate::universal_actions::action_ids::NOTE_OPEN,
        );
        let mut inspector_scope = scope();
        inspector_scope.surface = BindingEditorSurface::Inspector;
        let mut state = ActionBindingEditorState::default();
        state.reset_for(inspector_scope, Some(&original));
        state.tab = EditorTab::Advanced;
        state.query = "search used for Pin".into();
        state.query_mode = QueryRunMode::ExecuteFirst;
        state.command = "unassigned exact command".into();
        state.args = "unassigned arguments".into();
        state.stage_pinned_binding(pinned.clone());

        assert_eq!(state.active_binding, Some(original));
        assert!(state.has_unassigned_text());
        state.accept_committed_assignment(pinned.clone(), BindingAssignmentKind::Pinned);

        assert_eq!(state.active_binding, Some(pinned.clone()));
        assert_eq!(state.selected_binding(), Some(pinned));
        assert_eq!(state.query, "search used for Pin");
        assert_eq!(state.query_mode, QueryRunMode::ExecuteFirst);
        assert!(state.command.is_empty());
        assert!(state.args.is_empty());
        assert!(!state.has_unassigned_text());

        state.args = "a later exact command edit".into();
        assert!(state.has_unassigned_text());
    }

    #[test]
    fn request_pending_assignment_rejection_preserves_editor_buffers_and_pin_selection() {
        use crate::radial::authoring::menu;
        use crate::radial::authoring::{AuthoringError, AuthoringSnapshot, RadialAuthoringSession};
        use crate::radial::model::{CellContent, RadialDocument};

        let document = RadialDocument::starter();
        let (menu_id, ring_id, cell_id, original) = document
            .menus
            .iter()
            .find_map(|menu| {
                menu.rings.iter().find_map(|ring| {
                    ring.cells.iter().find_map(|cell| match &cell.content {
                        CellContent::Action { binding } => Some((
                            menu.id.clone(),
                            ring.id.clone(),
                            cell.id.clone(),
                            binding.clone(),
                        )),
                        _ => None,
                    })
                })
            })
            .expect("starter document has an authored action");
        let target = StableSelection::Cell {
            menu_id: menu_id.clone(),
            ring_id: ring_id.clone(),
            cell_id: cell_id.clone(),
        };
        let mut session = RadialAuthoringSession::new(AuthoringSnapshot::new(
            std::sync::Arc::new(document),
            "request-pending-assignment-fixture",
        ));
        session.select(Some(target.clone()));
        session.require_authoritative_snapshot();
        let scope = BindingEditorScope {
            surface: BindingEditorSurface::Inspector,
            editor_session: session.editor_session,
            draft_generation: session.generation,
            target,
            slot: BindingEditorSlot::CellPrimary,
        };
        let pinned = persisted_binding(
            crate::universal_actions::PersistableActionTargetRef::Note {
                slug: "rejected-pin-fixture".into(),
            },
            crate::universal_actions::action_ids::NOTE_OPEN,
        );
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope.clone(), Some(&original));
        state.query = "typed query to preserve".into();
        state.command = "window:close".into();
        state.args = "--all".into();
        state.tab = EditorTab::Query;
        state.search_rows = vec![
            crate::gui::universal_action_catalog::UniversalActionPickerRow::fixture(pinned.clone()),
        ];
        state.search_status = SearchStatus::Results;
        state.stage_pinned_binding(pinned.clone());

        let before_document = std::sync::Arc::clone(&session.draft);
        let before_generation = session.generation;
        let before_history = session.acceptance_history_depths();
        let mut direct_candidate = (*session.draft).clone();
        direct_candidate.menus[0].name.push_str(" pending edit");
        assert_eq!(
            session.replace_document_atomic(direct_candidate),
            Err(AuthoringError::RequestPending),
            "the authoritative snapshot guard rejects a real document edit"
        );
        assert!(session.is_initial_snapshot_pending());

        let context = egui::Context::default();
        context.enable_accesskit();
        let catalog = action_snapshot(Vec::new());
        let (initial, _) = render_editor_with_catalog(
            &context,
            &mut state,
            scope.clone(),
            &original,
            &catalog,
            Vec::new(),
        );

        let (after_pin, pin_intents) = click_action_editor_button(
            &context,
            &mut state,
            scope.clone(),
            &original,
            &catalog,
            &initial,
            "Pin this action",
        );
        let pin = pin_intents.into_iter().find_map(|intent| match intent {
            ActionBindingEditorIntent::Pin { binding } => Some(binding),
            _ => None,
        });
        assert_eq!(pin, Some(pinned.clone()));
        let rejected_pin = menu::set_cell_content(
            &mut session,
            &menu_id,
            &ring_id,
            &cell_id,
            CellContent::Action {
                binding: pinned.clone(),
            },
        );
        assert!(rejected_pin.is_err());
        assert!(
            state
                .finish_assignment(pinned.clone(), BindingAssignmentKind::Pinned, rejected_pin,)
                .is_err()
        );
        assert_eq!(state.active_binding, Some(original.clone()));
        assert_eq!(state.selected_binding(), Some(pinned.clone()));
        assert_eq!(state.query, "typed query to preserve");
        assert_eq!(state.command, "window:close");
        assert_eq!(state.args, "--all");

        let (_after_save, save_intents) = click_action_editor_button(
            &context,
            &mut state,
            scope.clone(),
            &original,
            &catalog,
            &after_pin,
            "Save query",
        );
        let saved_query = save_intents.into_iter().find_map(|intent| match intent {
            ActionBindingEditorIntent::SaveQuery { binding } => Some(binding),
            _ => None,
        });
        let saved_query = saved_query.expect("Save query click returns a staged assignment");
        let rejected_save = menu::set_cell_content(
            &mut session,
            &menu_id,
            &ring_id,
            &cell_id,
            CellContent::Action {
                binding: saved_query.clone(),
            },
        );
        assert!(rejected_save.is_err());
        assert!(
            state
                .finish_assignment(saved_query, BindingAssignmentKind::Explicit, rejected_save,)
                .is_err()
        );
        assert_eq!(state.active_binding, Some(original.clone()));

        state.tab = EditorTab::Advanced;
        let (advanced, _) = render_editor_with_catalog(
            &context,
            &mut state,
            scope.clone(),
            &original,
            &catalog,
            Vec::new(),
        );
        let (_after_command, command_intents) = click_action_editor_button(
            &context,
            &mut state,
            scope.clone(),
            &original,
            &catalog,
            &advanced,
            "Use exact command",
        );
        let exact_command = command_intents.into_iter().find_map(|intent| match intent {
            ActionBindingEditorIntent::SetCommand { binding } => Some(binding),
            _ => None,
        });
        let exact_command = exact_command.expect("Use exact command click stages a binding");
        let rejected_command = menu::set_cell_content(
            &mut session,
            &menu_id,
            &ring_id,
            &cell_id,
            CellContent::Action {
                binding: exact_command.clone(),
            },
        );
        assert!(rejected_command.is_err());
        assert!(
            state
                .finish_assignment(
                    exact_command,
                    BindingAssignmentKind::Explicit,
                    rejected_command,
                )
                .is_err()
        );
        assert_eq!(state.active_binding, Some(original.clone()));

        state.tab = EditorTab::Query;
        let (rendered, _) = render_editor_with_catalog(
            &context,
            &mut state,
            scope,
            &original,
            &catalog,
            Vec::new(),
        );
        assert!(rendered.platform_output.accesskit_update.is_some());
        assert_eq!(state.query, "typed query to preserve");
        assert_eq!(state.command, "window:close");
        assert_eq!(state.args, "--all");
        assert_eq!(state.active_binding, Some(original));
        assert_eq!(state.selected_binding(), Some(pinned));
        assert!(state.has_unassigned_text());
        let controls = crate::radial::acceptance_trace::take_action_editor_control_test_events();
        assert!(controls.iter().any(|event| matches!(
            event,
            crate::radial::acceptance_trace::Event::DesignerActionEditorControl {
                control: "result_target",
                selected: true,
                ..
            }
        )));
        assert_eq!(&*session.draft, &*before_document);
        assert_eq!(session.generation, before_generation);
        assert_eq!(session.acceptance_history_depths(), before_history);
        assert!(!session.is_dirty());
    }
    use std::sync::{Arc, Mutex, mpsc};

    fn frame(
        context: &egui::Context,
        state: &mut ActionBindingEditorState,
        scope: BindingEditorScope,
        binding: &ActionBinding,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        frame_at_monitor_origin(context, state, scope, binding, events, egui::Pos2::ZERO)
    }

    fn frame_at_monitor_origin(
        context: &egui::Context,
        state: &mut ActionBindingEditorState,
        scope: BindingEditorScope,
        binding: &ActionBinding,
        events: Vec<egui::Event>,
        monitor_origin: egui::Pos2,
    ) -> egui::FullOutput {
        frame_at_monitor_origin_with_clip(
            context,
            state,
            scope,
            binding,
            events,
            monitor_origin,
            None,
        )
    }

    fn frame_at_monitor_origin_with_clip(
        context: &egui::Context,
        state: &mut ActionBindingEditorState,
        scope: BindingEditorScope,
        binding: &ActionBinding,
        events: Vec<egui::Event>,
        monitor_origin: egui::Pos2,
        clip_rect: Option<egui::Rect>,
    ) -> egui::FullOutput {
        let client_size = egui::vec2(900.0, 520.0);
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, client_size)),
            events,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("root viewport is present in RawInput")
            .inner_rect = Some(egui::Rect::from_min_size(monitor_origin, client_size));
        context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                if let Some(clip_rect) = clip_rect {
                    ui.set_clip_rect(clip_rect);
                }
                let _ = state.show(ui, scope, Some(binding), &InvocationContext::empty(0));
            });
        })
    }

    fn frame_in_narrow_panel(
        context: &egui::Context,
        state: &mut ActionBindingEditorState,
        scope: BindingEditorScope,
        binding: &ActionBinding,
        panel_width: f32,
    ) -> egui::FullOutput {
        let client_size = egui::vec2(900.0, 520.0);
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, client_size)),
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("root viewport is present in RawInput")
            .inner_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, client_size));
        context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let height = ui.available_height();
                ui.allocate_ui(egui::vec2(panel_width, height), |ui| {
                    let _ = state.show(ui, scope, Some(binding), &InvocationContext::empty(0));
                });
            });
        })
    }

    struct NestedInspectorFrame {
        output: egui::FullOutput,
        outer_offset_y: f32,
        outer_content_height: f32,
        outer_inner_height: f32,
        outer_bar_rects: Option<(egui::Rect, egui::Rect)>,
        intents: Vec<ActionBindingEditorIntent>,
    }

    #[derive(Clone, Copy, Debug)]
    struct ActionEditorScrollGeometry {
        offset_y_milli: i64,
        pixels_per_point_milli: i64,
        inner: [i32; 4],
        track: [i32; 4],
        track_visible: [i32; 4],
        thumb: [i32; 4],
        thumb_visible: [i32; 4],
        painted_thumb: [i32; 4],
        painted_thumb_visible: [i32; 4],
        client_size: [i32; 2],
    }

    fn action_editor_scroll_geometry(
        events: &[crate::radial::acceptance_trace::Event],
    ) -> Option<ActionEditorScrollGeometry> {
        events.iter().rev().find_map(|event| match event {
            crate::radial::acceptance_trace::Event::DesignerActionEditorScroll {
                offset_y_milli,
                pixels_per_point_milli,
                inner_left_px,
                inner_top_px,
                inner_right_px,
                inner_bottom_px,
                track_left_px,
                track_top_px,
                track_right_px,
                track_bottom_px,
                track_visible_left_px,
                track_visible_top_px,
                track_visible_right_px,
                track_visible_bottom_px,
                thumb_left_px,
                thumb_top_px,
                thumb_right_px,
                thumb_bottom_px,
                thumb_visible_left_px,
                thumb_visible_top_px,
                thumb_visible_right_px,
                thumb_visible_bottom_px,
                painted_thumb_left_px,
                painted_thumb_top_px,
                painted_thumb_right_px,
                painted_thumb_bottom_px,
                painted_thumb_visible_left_px,
                painted_thumb_visible_top_px,
                painted_thumb_visible_right_px,
                painted_thumb_visible_bottom_px,
                client_width_px,
                client_height_px,
                ..
            } => Some(ActionEditorScrollGeometry {
                offset_y_milli: *offset_y_milli,
                pixels_per_point_milli: *pixels_per_point_milli,
                inner: [
                    *inner_left_px,
                    *inner_top_px,
                    *inner_right_px,
                    *inner_bottom_px,
                ],
                track: [
                    *track_left_px,
                    *track_top_px,
                    *track_right_px,
                    *track_bottom_px,
                ],
                track_visible: [
                    *track_visible_left_px,
                    *track_visible_top_px,
                    *track_visible_right_px,
                    *track_visible_bottom_px,
                ],
                thumb: [
                    *thumb_left_px,
                    *thumb_top_px,
                    *thumb_right_px,
                    *thumb_bottom_px,
                ],
                thumb_visible: [
                    *thumb_visible_left_px,
                    *thumb_visible_top_px,
                    *thumb_visible_right_px,
                    *thumb_visible_bottom_px,
                ],
                painted_thumb: [
                    *painted_thumb_left_px,
                    *painted_thumb_top_px,
                    *painted_thumb_right_px,
                    *painted_thumb_bottom_px,
                ],
                painted_thumb_visible: [
                    *painted_thumb_visible_left_px,
                    *painted_thumb_visible_top_px,
                    *painted_thumb_visible_right_px,
                    *painted_thumb_visible_bottom_px,
                ],
                client_size: [*client_width_px, *client_height_px],
            }),
            _ => None,
        })
    }

    fn nested_inspector_frame(
        context: &egui::Context,
        state: &mut ActionBindingEditorState,
        scope: BindingEditorScope,
        binding: &ActionBinding,
        time: f64,
        events: Vec<egui::Event>,
    ) -> NestedInspectorFrame {
        let client_size = egui::vec2(900.0, 650.0);
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, client_size)),
            time: Some(time),
            events,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("root viewport is present in RawInput")
            .inner_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, client_size));

        let mut outer_offset_y = 0.0;
        let mut outer_content_height = 0.0;
        let mut outer_inner_height = 0.0;
        let mut outer_bar_rects = None;
        let mut intents = Vec::new();
        let output = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let panel_rect = ui.max_rect();
                let inspector_width = 314.0_f32.min(client_size.x * 0.45);
                let inspector_rect = egui::Rect::from_min_size(
                    egui::pos2(client_size.x - inspector_width, panel_rect.top() + 8.0),
                    egui::vec2(inspector_width, 560.0),
                );
                ui.allocate_ui_at_rect(inspector_rect, |ui| {
                    let outer = egui::ScrollArea::vertical()
                        .id_source("radial-designer-inspector")
                        .max_height(560.0)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            ui.heading("Inspector");
                            // Keep the query editor at the lower position of a
                            // populated Inspector, where the result viewport is
                            // constrained to the native 64pt acceptance geometry.
                            for index in 0..24 {
                                ui.small(format!("Cell property {index}"));
                            }
                            intents.extend(state.show(
                                ui,
                                scope,
                                Some(binding),
                                &InvocationContext::empty(0),
                            ));
                            for index in 0..10 {
                                ui.small(format!("Inspector continuation {index}"));
                            }
                        });
                    outer_offset_y = outer.state.offset.y;
                    outer_content_height = outer.content_size.y;
                    outer_inner_height = outer.inner_rect.height();
                    outer_bar_rects = context
                        .read_response(outer.id.with(1))
                        .map(|response| (response.rect, response.interact_rect));
                });
            });
        });
        NestedInspectorFrame {
            output,
            outer_offset_y,
            outer_content_height,
            outer_inner_height,
            outer_bar_rects,
            intents,
        }
    }

    fn input_bounds(output: &egui::FullOutput, value: &str) -> egui::Rect {
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("AccessKit update");
        let (_, node) = update
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::TextInput && node.value() == Some(value)
            })
            .unwrap_or_else(|| panic!("missing TextInput with value {value:?}"));
        let bounds = node.bounds().expect("text input bounds");
        egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
        )
    }

    fn pointer_button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn replace_text(
        context: &egui::Context,
        state: &mut ActionBindingEditorState,
        scope: &BindingEditorScope,
        binding: &ActionBinding,
        output: &egui::FullOutput,
        existing: &str,
        replacement: &str,
    ) -> egui::FullOutput {
        let point = input_bounds(output, existing).center();
        let _ = frame(
            context,
            state,
            scope.clone(),
            binding,
            vec![egui::Event::PointerMoved(point)],
        );
        let _ = frame(
            context,
            state,
            scope.clone(),
            binding,
            vec![pointer_button(point, true)],
        );
        let _ = frame(
            context,
            state,
            scope.clone(),
            binding,
            vec![pointer_button(point, false)],
        );
        let move_to_start = egui::Event::Key {
            key: egui::Key::Home,
            physical_key: Some(egui::Key::Home),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        };
        let _ = frame(context, state, scope.clone(), binding, vec![move_to_start]);
        let select_to_end = egui::Event::Key {
            key: egui::Key::End,
            physical_key: Some(egui::Key::End),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        let _ = frame(context, state, scope.clone(), binding, vec![select_to_end]);
        let _ = frame(
            context,
            state,
            scope.clone(),
            binding,
            vec![egui::Event::Text(replacement.to_owned())],
        );
        frame(context, state, scope.clone(), binding, Vec::new())
    }

    fn scope() -> BindingEditorScope {
        BindingEditorScope {
            surface: BindingEditorSurface::Properties,
            editor_session: AuthoringSessionId(7),
            draft_generation: DraftGeneration(3),
            target: StableSelection::Cell {
                menu_id: crate::radial::model::MenuId::new("main"),
                ring_id: crate::radial::model::RingId::new("root"),
                cell_id: crate::radial::model::CellId::new("cell-a"),
            },
            slot: BindingEditorSlot::CellPrimary,
        }
    }

    fn persisted_binding(
        target: crate::universal_actions::PersistableActionTargetRef,
        action_id: crate::universal_actions::ActionId,
    ) -> ActionBinding {
        ActionBinding::Persisted {
            action: crate::universal_actions::PersistedUniversalActionRef {
                target: Some(target),
                action_id,
            },
        }
    }

    fn action_snapshot(
        entries: Vec<crate::universal_actions::ResolvedActionTarget>,
    ) -> crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot {
        crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot {
            entries,
            recent_entries: Vec::new(),
            dashboard: std::sync::Arc::new(crate::dashboard::DashboardDataSnapshot::default()),
        }
    }

    fn render_editor_with_catalog(
        context: &egui::Context,
        state: &mut ActionBindingEditorState,
        editor_scope: BindingEditorScope,
        binding: &ActionBinding,
        action_catalog: &crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<ActionBindingEditorIntent>) {
        let size = egui::vec2(900.0, 520.0);
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("root viewport is present in RawInput")
            .inner_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size));
        let mut intents = Vec::new();
        let output = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                intents = state.show_with_action_catalog(
                    ui,
                    editor_scope,
                    Some(binding),
                    &InvocationContext::empty(0),
                    Some(action_catalog),
                );
            });
        });
        (output, intents)
    }

    fn click_action_editor_button(
        context: &egui::Context,
        state: &mut ActionBindingEditorState,
        editor_scope: BindingEditorScope,
        binding: &ActionBinding,
        action_catalog: &crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot,
        output: &egui::FullOutput,
        name: &str,
    ) -> (egui::FullOutput, Vec<ActionBindingEditorIntent>) {
        let point = named_button_bounds(output, name).center();
        let mut latest = None;
        let mut intents = Vec::new();
        for event in [
            egui::Event::PointerMoved(point),
            pointer_button(point, true),
            pointer_button(point, false),
        ] {
            let (output, emitted) = render_editor_with_catalog(
                context,
                state,
                editor_scope.clone(),
                binding,
                action_catalog,
                vec![event],
            );
            latest = Some(output);
            intents.extend(emitted);
        }
        (
            latest.expect("button click renders at least one frame"),
            intents,
        )
    }

    fn render_assigned_presentation(
        context: &egui::Context,
        state: &mut ActionBindingEditorState,
        _editor_scope: BindingEditorScope,
        binding: &ActionBinding,
        presentation: (String, Option<String>),
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<ActionBindingEditorIntent>) {
        let size = egui::vec2(900.0, 520.0);
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("root viewport is present in RawInput")
            .inner_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size));
        let mut intents = Vec::new();
        let output = context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                state.assigned_binding_ui(ui, binding, presentation, &mut intents);
            });
        });
        (output, intents)
    }

    fn assert_assigned_test_unavailable(
        context: &egui::Context,
        state: &mut ActionBindingEditorState,
        editor_scope: BindingEditorScope,
        binding: &ActionBinding,
        presentation: (String, Option<String>),
        render: impl Fn(
            &egui::Context,
            &mut ActionBindingEditorState,
            BindingEditorScope,
            &ActionBinding,
            (String, Option<String>),
            Vec<egui::Event>,
        ) -> (egui::FullOutput, Vec<ActionBindingEditorIntent>),
    ) -> String {
        let _ = crate::radial::acceptance_trace::take_action_editor_control_test_events();
        let (output, intents) = render(
            context,
            state,
            editor_scope.clone(),
            binding,
            presentation.clone(),
            Vec::new(),
        );
        assert!(intents.is_empty());
        let control = crate::radial::acceptance_trace::take_action_editor_control_test_events()
            .into_iter()
            .find_map(|event| match event {
                crate::radial::acceptance_trace::Event::DesignerActionEditorControl {
                    control: "test_assigned",
                    full_left_px,
                    full_top_px,
                    full_right_px,
                    full_bottom_px,
                    enabled,
                    ..
                } => Some((
                    [full_left_px, full_top_px, full_right_px, full_bottom_px],
                    enabled,
                )),
                _ => None,
            })
            .expect("assigned Test control was rendered");
        assert!(!control.1, "an unavailable assigned binding disables Test");
        let rendered_text = format!("{:?}", output.shapes);
        assert!(rendered_text.contains("Unavailable:"), "{rendered_text}");
        let reason = presentation
            .1
            .as_deref()
            .expect("test fixture must include the unavailable reason")
            .to_owned();
        assert!(rendered_text.contains(&reason), "{rendered_text}");

        let position = egui::pos2(
            (control.0[0] + control.0[2]) as f32 / 2.0,
            (control.0[1] + control.0[3]) as f32 / 2.0,
        );
        let mut all_intents = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(position)],
            vec![pointer_button(position, true)],
            vec![pointer_button(position, false)],
        ] {
            let (_, frame_intents) = render(
                context,
                state,
                editor_scope.clone(),
                binding,
                presentation.clone(),
                events,
            );
            all_intents.extend(frame_intents);
            let _ = crate::radial::acceptance_trace::take_action_editor_control_test_events();
        }
        assert!(
            all_intents
                .iter()
                .all(|intent| !matches!(intent, ActionBindingEditorIntent::Test { .. })),
            "the disabled assigned Test control cannot create a Test intent"
        );
        reason
    }

    fn generic_resolved_target(
        selected: &crate::actions::Action,
    ) -> crate::universal_actions::ResolvedActionTarget {
        crate::universal_actions::ResolvedActionTarget {
            target: crate::universal_actions::ActionTarget::Generic {
                action: selected.clone(),
            },
            selected_action: selected.clone(),
            custom_action_index: None,
        }
    }

    fn note_resolved_target(slug: &str) -> crate::universal_actions::ResolvedActionTarget {
        crate::universal_actions::ResolvedActionTarget {
            target: crate::universal_actions::ActionTarget::Note { slug: slug.into() },
            selected_action: crate::actions::Action {
                label: "Shared title note".into(),
                desc: "Note".into(),
                action: format!("note:open:{slug}"),
                args: None,
            },
            custom_action_index: None,
        }
    }

    #[test]
    fn assigned_duplicate_note_pins_show_slug_identity_on_both_surfaces() {
        let context = egui::Context::default();
        let slugs = ["same-title-note-a", "same-title-note-b"];
        let catalog = action_snapshot(
            slugs
                .iter()
                .map(|slug| note_resolved_target(slug))
                .collect(),
        );

        for slug in slugs {
            let binding = persisted_binding(
                crate::universal_actions::PersistableActionTargetRef::Note { slug: slug.into() },
                crate::universal_actions::action_ids::NOTE_EDIT,
            );
            let presentation = assigned_binding_presentation(&binding, Some(&catalog));
            let expected = format!("Shared title note · Note · slug {slug} — Edit Note");
            assert_eq!(presentation.0, expected);
            assert_eq!(presentation.1, None);

            for surface in [
                BindingEditorSurface::Properties,
                BindingEditorSurface::Inspector,
            ] {
                let mut editor_scope = scope();
                editor_scope.surface = surface;
                let mut state = ActionBindingEditorState::default();
                state.reset_for(editor_scope.clone(), Some(&binding));
                let (output, intents) = render_assigned_presentation(
                    &context,
                    &mut state,
                    editor_scope,
                    &binding,
                    presentation.clone(),
                    Vec::new(),
                );
                assert!(intents.is_empty());
                assert!(format!("{:?}", output.shapes).contains(&expected));
            }
        }
    }

    #[test]
    fn missing_persisted_pins_are_unavailable_and_not_testable_on_both_surfaces() {
        let context = egui::Context::default();
        let app = crate::gui::actions::tests::new_app(&context);
        let history_before = serde_json::to_vec(
            &crate::history::with_history(Clone::clone).expect("history snapshot"),
        )
        .expect("serialize history");
        let usage_before = app.usage.clone();
        let activation_before = app.test_activation_trace.clone();
        let selected = test_action("help:show", "Missing pin fixture");
        let live_target = generic_resolved_target(&selected);
        let live_reference = live_target
            .target
            .persistent_ref()
            .expect("generic action is persistable");
        let missing_target = persisted_binding(
            crate::universal_actions::PersistableActionTargetRef::Note {
                slug: "deleted-note".into(),
            },
            crate::universal_actions::action_ids::NOTE_EDIT,
        );
        let missing_action = persisted_binding(
            live_reference,
            crate::universal_actions::action_ids::NOTE_EDIT,
        );
        let empty_catalog = action_snapshot(Vec::new());
        let target_catalog = action_snapshot(vec![live_target]);

        for surface in [
            BindingEditorSurface::Properties,
            BindingEditorSurface::Inspector,
        ] {
            for (binding, catalog, expected_reason) in [
                (
                    &missing_target,
                    &empty_catalog,
                    "saved note no longer exists",
                ),
                (
                    &missing_action,
                    &target_catalog,
                    "action note.edit is unavailable for the saved target",
                ),
            ] {
                let mut editor_scope = scope();
                editor_scope.surface = surface;
                let mut state = ActionBindingEditorState::default();
                state.reset_for(editor_scope.clone(), Some(binding));
                let (_, reason) = assigned_binding_presentation(binding, Some(catalog));
                assert_eq!(reason.as_deref(), Some(expected_reason));
                let render = |context: &egui::Context,
                              state: &mut ActionBindingEditorState,
                              editor_scope: BindingEditorScope,
                              binding: &ActionBinding,
                              _presentation: (String, Option<String>),
                              events: Vec<egui::Event>| {
                    render_editor_with_catalog(
                        context,
                        state,
                        editor_scope,
                        binding,
                        catalog,
                        events,
                    )
                };
                assert_eq!(
                    assert_assigned_test_unavailable(
                        &context,
                        &mut state,
                        editor_scope,
                        binding,
                        assigned_binding_presentation(binding, Some(catalog)),
                        render,
                    ),
                    expected_reason,
                );
            }
        }

        assert_eq!(
            serde_json::to_vec(
                &crate::history::with_history(Clone::clone).expect("history snapshot"),
            )
            .expect("serialize history"),
            history_before,
        );
        assert_eq!(app.usage, usage_before);
        assert_eq!(app.test_activation_trace, activation_before);
        assert!(app.test_recorded_history_queries.is_empty());
    }

    #[test]
    fn disabled_persisted_pin_is_unavailable_and_not_testable_on_both_surfaces() {
        let context = egui::Context::default();
        let app = crate::gui::actions::tests::new_app(&context);
        let history_before = serde_json::to_vec(
            &crate::history::with_history(Clone::clone).expect("history snapshot"),
        )
        .expect("serialize history");
        let usage_before = app.usage.clone();
        let activation_before = app.test_activation_trace.clone();
        let target = note_resolved_target("disabled-pin-note");
        let reference = crate::universal_actions::PersistedUniversalActionRef {
            target: target.target.persistent_ref(),
            action_id: crate::universal_actions::action_ids::NOTE_EDIT,
        };
        let mut resolved_action = crate::universal_actions::UniversalActionRegistry
            .resolve(
                &target,
                &crate::universal_actions::ActionResolutionContext::new(
                    crate::universal_actions::ActionSurface::RadialMenu,
                    "",
                ),
            )
            .into_iter()
            .find(|action| action.id == reference.action_id)
            .expect("generic result action");
        let unavailable_reason = "fixture note action was disabled by the live provider";
        resolved_action.availability = crate::universal_actions::ActionAvailability::Disabled {
            reason: unavailable_reason.into(),
        };
        let binding = ActionBinding::Persisted {
            action: reference.clone(),
        };
        let resolution = crate::universal_actions::ResolvedPersistedAction {
            reference,
            target,
            action: resolved_action,
        };
        let presentation = assigned_persisted_action_presentation(&binding, Ok(resolution));
        assert_eq!(presentation.1.as_deref(), Some(unavailable_reason));
        assert_eq!(
            presentation.0,
            "Shared title note · Note · slug disabled-pin-note — Edit Note"
        );

        for surface in [
            BindingEditorSurface::Properties,
            BindingEditorSurface::Inspector,
        ] {
            let mut editor_scope = scope();
            editor_scope.surface = surface;
            let mut state = ActionBindingEditorState::default();
            state.reset_for(editor_scope.clone(), Some(&binding));
            assert_eq!(
                assert_assigned_test_unavailable(
                    &context,
                    &mut state,
                    editor_scope,
                    &binding,
                    presentation.clone(),
                    render_assigned_presentation,
                ),
                unavailable_reason,
            );
        }

        assert_eq!(
            serde_json::to_vec(
                &crate::history::with_history(Clone::clone).expect("history snapshot"),
            )
            .expect("serialize history"),
            history_before,
        );
        assert_eq!(app.usage, usage_before);
        assert_eq!(app.test_activation_trace, activation_before);
        assert!(app.test_recorded_history_queries.is_empty());
    }

    fn exact_binding() -> ActionBinding {
        ActionBinding::ExactCommand {
            command: "window:close".into(),
            args: Some("first argument".into()),
        }
    }

    struct AuthoringSearchPlugin {
        results: Vec<crate::actions::Action>,
        searched: Option<mpsc::Sender<String>>,
    }

    impl crate::plugin::Plugin for AuthoringSearchPlugin {
        fn search(&self, query: &str) -> Vec<crate::actions::Action> {
            if let Some(searched) = &self.searched {
                let _ = searched.send(query.to_owned());
            }
            self.results.clone()
        }

        fn name(&self) -> &str {
            "m4_authoring_search_fixture"
        }

        fn description(&self) -> &str {
            "M4 authoring search test fixture"
        }

        fn capabilities(&self) -> &[&str] {
            &["search"]
        }

        fn always_search(&self) -> bool {
            true
        }
    }

    struct CloseThenSearchPlugin {
        first_started: mpsc::Sender<()>,
        release_first: Mutex<mpsc::Receiver<()>>,
        latest_started: mpsc::Sender<()>,
        result: crate::actions::Action,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl crate::plugin::Plugin for CloseThenSearchPlugin {
        fn search(&self, _query: &str) -> Vec<crate::actions::Action> {
            use std::sync::atomic::Ordering;
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                let _ = self.first_started.send(());
                let _ = self.release_first.lock().unwrap().recv();
                // A canceled provider is deliberately allowed to return a
                // result. The worker must discard it after releasing capacity.
                vec![self.result.clone()]
            } else {
                let _ = self.latest_started.send(());
                vec![self.result.clone()]
            }
        }

        fn name(&self) -> &str {
            "m4_close_then_latest_search_fixture"
        }

        fn description(&self) -> &str {
            "M4 canceled-worker queue progress fixture"
        }

        fn capabilities(&self) -> &[&str] {
            &["search"]
        }

        fn always_search(&self) -> bool {
            true
        }
    }

    struct PanickingSearchPlugin;

    impl crate::plugin::Plugin for PanickingSearchPlugin {
        fn search(&self, _query: &str) -> Vec<crate::actions::Action> {
            panic!("M4 test provider failure");
        }

        fn name(&self) -> &str {
            "m4_authoring_provider_failure_fixture"
        }

        fn description(&self) -> &str {
            "M4 provider failure test fixture"
        }

        fn capabilities(&self) -> &[&str] {
            &["search"]
        }

        fn always_search(&self) -> bool {
            true
        }
    }

    struct BlockedRevisionSearchPlugin {
        started: mpsc::Sender<usize>,
        release: Mutex<mpsc::Receiver<()>>,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl crate::plugin::Plugin for BlockedRevisionSearchPlugin {
        fn search(&self, _query: &str) -> Vec<crate::actions::Action> {
            use std::sync::atomic::Ordering;
            let attempt = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = self.started.send(attempt);
            let _ = self.release.lock().unwrap().recv();
            Vec::new()
        }

        fn name(&self) -> &str {
            "m4_authoring_revision_retry_fixture"
        }

        fn description(&self) -> &str {
            "M4 provider revision retry test fixture"
        }

        fn capabilities(&self) -> &[&str] {
            &["search"]
        }

        fn always_search(&self) -> bool {
            true
        }
    }

    fn test_action(command: &str, label: &str) -> crate::actions::Action {
        crate::actions::Action {
            label: label.into(),
            desc: "M4 authoring fixture".into(),
            action: command.into(),
            args: None,
        }
    }

    fn install_properties_editor(
        app: &mut crate::gui::LauncherApp,
        binding: ActionBinding,
        query: &str,
        dirty_session: bool,
        test_request: bool,
    ) -> ActionBindingEditorIntent {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        if dirty_session {
            editor.make_dirty_for_test();
        }
        let (target, scope) = {
            let session = editor.session.as_ref().expect("test authoring session");
            let menu = session.draft.menus.first().expect("starter menu");
            let ring = menu.rings.first().expect("starter ring");
            let cell = ring.cells.first().expect("starter cell");
            let target = StableSelection::Cell {
                menu_id: menu.id.clone(),
                ring_id: ring.id.clone(),
                cell_id: cell.id.clone(),
            };
            let scope = BindingEditorScope {
                surface: BindingEditorSurface::Properties,
                editor_session: session.editor_session,
                draft_generation: session.generation,
                target: target.clone(),
                slot: BindingEditorSlot::CellPrimary,
            };
            (target, scope)
        };
        let cell = editor.session.as_ref().unwrap().draft.menus[0].rings[0].cells[0].clone();
        let mut draft = PropertiesDraft::from_cell(target.clone(), scope.draft_generation, &cell);
        draft.content_kind = 1;
        draft.action_binding = Some(binding.clone());
        draft.action_editor.reset_for(scope, Some(&binding));
        draft.action_editor.query = query.to_owned();
        let mut intents = Vec::new();
        if test_request {
            draft.action_editor.emit_test(binding, &mut intents);
        } else {
            draft.action_editor.emit_search(&mut intents);
        }
        let intent = intents.into_iter().next().expect("editor intent");
        assert!(matches!(
            (&intent, test_request),
            (ActionBindingEditorIntent::Test { .. }, true)
                | (ActionBindingEditorIntent::Search { .. }, false)
        ));
        editor.properties_popup = Some(target);
        editor.properties_draft = Some(draft);
        *app.radial_editor.lock().expect("radial editor lock") = editor;
        intent
    }

    fn route_editor_intent(app: &mut crate::gui::LauncherApp, intent: ActionBindingEditorIntent) {
        let bridge = app
            .radial_editor
            .lock()
            .expect("radial editor lock")
            .intent_bridge();
        match intent {
            ActionBindingEditorIntent::Search { identity, query } => {
                bridge.push(crate::gui::radial_editor::DesignerUiIntent::ActionSearch {
                    identity,
                    query,
                });
            }
            ActionBindingEditorIntent::Test {
                identity,
                binding,
                history_query,
            } => {
                bridge.push(crate::gui::radial_editor::DesignerUiIntent::TestAction {
                    identity,
                    binding,
                    invocation: InvocationContext::empty(0),
                    history_query,
                });
            }
            other => panic!("expected Search or Test intent, received {other:?}"),
        }
        app.process_radial_designer_intents();
    }

    fn wait_for_authoring_state(
        app: &mut crate::gui::LauncherApp,
        mut finished: impl FnMut(&crate::gui::LauncherApp) -> bool,
    ) {
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        while Instant::now() < deadline {
            app.process_watch_events();
            if finished(app) {
                return;
            }
            std::thread::yield_now();
        }
        app.process_watch_events();
        assert!(
            finished(app),
            "authoring provider did not reach a terminal state"
        );
    }

    fn receive_authoring_search_result(
        app: &mut crate::gui::LauncherApp,
    ) -> crate::gui::WatchEvent {
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        while Instant::now() < deadline {
            match app.rx.try_recv() {
                Ok(event) => {
                    app.event_sink.event_consumed();
                    match event {
                        crate::gui::WatchEvent::AuthoringProviderCapacityAvailable => {
                            app.start_next_authoring_provider_search();
                        }
                        event @ crate::gui::WatchEvent::RadialAuthoringSearchReady { .. }
                        | event @ crate::gui::WatchEvent::RadialAuthoringSearchFailed { .. } => {
                            return event;
                        }
                        _ => {
                            panic!("unexpected non-authoring event while awaiting authoring search")
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => std::thread::yield_now(),
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("authoring event channel disconnected")
                }
            }
        }
        panic!("authoring provider did not enqueue its terminal event");
    }

    fn deliver_watch_event(app: &mut crate::gui::LauncherApp, event: crate::gui::WatchEvent) {
        app.event_tx
            .send(event)
            .expect("the test app event receiver remains open");
        app.process_watch_events();
    }

    fn install_window_target(app: &mut crate::gui::LauncherApp) {
        let descriptor = crate::window_catalog::WindowDescriptor {
            title: "M4 Editor Fixture".into(),
            hwnd: 44,
            pid: 7,
            executable: Some("editor.exe".into()),
            process_path: Some("C:\\Apps\\editor.exe".into()),
            class_name: Some("EditorWindow".into()),
        };
        let current = descriptor.clone();
        app.plugins.set_window_catalog_for_test(
            crate::window_catalog::WindowCatalog::from_snapshot_with_descriptor(
                vec![descriptor],
                move |hwnd| (hwnd == current.hwnd).then(|| current.clone()),
            ),
        );
    }

    fn prompt_frame(
        context: &egui::Context,
        editor: &mut RadialEditorState,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |context| editor.prompts(context),
        )
    }

    fn named_button_bounds(output: &egui::FullOutput, name: &str) -> egui::Rect {
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("prompt AccessKit update");
        let (_, node) = update
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Button && node.name() == Some(name)
            })
            .unwrap_or_else(|| panic!("missing close prompt button {name:?}"));
        let bounds = node.bounds().expect("button bounds");
        egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
        )
    }

    fn click_button(
        context: &egui::Context,
        editor: &mut RadialEditorState,
        output: &egui::FullOutput,
        name: &str,
    ) {
        let point = named_button_bounds(output, name).center();
        let _ = prompt_frame(context, editor, vec![egui::Event::PointerMoved(point)]);
        let _ = prompt_frame(context, editor, vec![pointer_button(point, true)]);
        let _ = prompt_frame(context, editor, vec![pointer_button(point, false)]);
    }

    #[test]
    fn command_edits_retire_test_but_do_not_strand_preview_completion() {
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope(), Some(&exact_binding()));
        let mut intents = Vec::new();
        state.emit_search(&mut intents);
        let ActionBindingEditorIntent::Search {
            identity: search, ..
        } = intents.remove(0)
        else {
            panic!("expected search intent")
        };

        state.emit_test(exact_binding(), &mut intents);
        let ActionBindingEditorIntent::Test {
            identity: test,
            binding,
            ..
        } = intents.remove(0)
        else {
            panic!("expected explicit Test intent")
        };
        assert!(state.test_binding_matches(&test, &binding));

        // Test-only identity changes and edit-only changes are independent of
        // the outstanding preview correlation.
        assert!(state.matches_search_identity(&search));
        state.query_mode = QueryRunMode::ExecuteFirst;
        state.bump_edit_generation();
        assert!(state.matches_search_identity(&search));
        assert!(!state.test_binding_matches(&test, &binding));

        let completion = ActionSearchCompletion {
            identity: search,
            state: ActionSearchCompletionState::Pending,
            rows: Vec::new(),
            preview: None,
            preview_unavailable: None,
        };
        assert!(state.complete_search(completion));
        assert_eq!(state.search_status, SearchStatus::Pending);
        assert!(state.pending_search_due.is_some());
    }

    #[test]
    fn repeated_searches_have_distinct_request_identities() {
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope(), None);
        let mut intents = Vec::new();
        state.emit_search(&mut intents);
        let ActionBindingEditorIntent::Search {
            identity: first, ..
        } = intents.remove(0)
        else {
            panic!("expected first search intent")
        };
        state.emit_search(&mut intents);
        let ActionBindingEditorIntent::Search {
            identity: second, ..
        } = intents.remove(0)
        else {
            panic!("expected second search intent")
        };
        assert_ne!(
            first.search_request_generation,
            second.search_request_generation
        );
        assert!(!state.matches_search_identity(&first));
        assert!(state.matches_search_identity(&second));
    }

    #[test]
    fn retirement_tracks_the_issued_search_query_not_newer_editor_text() {
        let _ = crate::radial::acceptance_trace::take_authoring_provider_trace_test_events();
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope(), None);
        state.query = "issued query".into();
        let mut intents = Vec::new();
        state.emit_search(&mut intents);
        let ActionBindingEditorIntent::Search {
            identity: issued_identity,
            query: issued_query,
        } = intents.remove(0)
        else {
            panic!("expected Search intent")
        };
        state.query = "newer unsaved text".into();
        state.query_generation = state.query_generation.wrapping_add(1).max(1);
        state.invalidate_pending_requests();

        assert_eq!(issued_query, "issued query");
        assert_ne!(state.identity().as_ref(), Some(&issued_identity));
        assert!(state.pending_search_identity.is_none());
        assert!(state.pending_search_query.is_none());
        let events = crate::radial::acceptance_trace::take_authoring_provider_trace_test_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].edge, "retired");
        assert_eq!(events[0].kind, "search");
        assert_eq!(events[0].identity, issued_identity);
        assert_eq!(
            events[0].query_digest,
            crate::radial::acceptance_trace::private_trace_text_digest("issued query")
        );
    }

    #[test]
    fn assigned_query_retirement_uses_binding_query_and_keeps_history_query() {
        let _ = crate::radial::acceptance_trace::take_authoring_provider_trace_test_events();
        let binding = ActionBinding::LauncherQuery {
            query: "saved query A".into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        let scope = scope();
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope.clone(), Some(&binding));
        let initial = frame(&context, &mut state, scope.clone(), &binding, Vec::new());
        let edited = replace_text(
            &context,
            &mut state,
            &scope,
            &binding,
            &initial,
            "saved query A",
            "unsaved query B",
        );
        assert_eq!(state.query, "unsaved query B");

        let mut intents = Vec::new();
        state.emit_test(binding.clone(), &mut intents);
        let ActionBindingEditorIntent::Test {
            identity,
            history_query,
            ..
        } = intents.remove(0)
        else {
            panic!("expected Test intent")
        };
        assert_eq!(history_query, "unsaved query B");
        assert_eq!(state.pending_test_query.as_deref(), Some("saved query A"));
        assert_eq!(state.pending_test_identity.as_ref(), Some(&identity));

        state.invalidate_pending_requests();
        assert!(state.pending_test_identity.is_none());
        assert!(state.pending_test_query.is_none());
        assert!(edited.platform_output.accesskit_update.is_some());
        let events = crate::radial::acceptance_trace::take_authoring_provider_trace_test_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].edge, "retired");
        assert_eq!(events[0].kind, "test");
        assert_eq!(events[0].identity, identity);
        assert_eq!(
            events[0].query_digest,
            crate::radial::acceptance_trace::private_trace_text_digest("saved query A")
        );
        assert_eq!(
            events[0].binding_digest,
            trace_binding_digest(Some(&binding))
        );
    }

    #[test]
    fn debounced_pending_preview_has_no_request_identity_to_retire() {
        let _ = crate::radial::acceptance_trace::take_authoring_provider_trace_test_events();
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope(), None);
        state.search_status = SearchStatus::Pending;
        state.pending_search_due = Some(Instant::now() + SEARCH_DEBOUNCE);
        assert!(state.pending_search_identity.is_none());
        state.invalidate_pending_requests();
        assert!(state.pending_search_identity.is_none());
        assert!(state.pending_search_query.is_none());
        assert!(
            crate::radial::acceptance_trace::take_authoring_provider_trace_test_events().is_empty()
        );
    }

    #[test]
    fn close_invalidates_pending_confirmation_without_clearing_editor_buffers() {
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope(), Some(&exact_binding()));
        state.command = "window:show".into();
        state.args = "unsaved argument draft".into();
        let mut intents = Vec::new();
        state.emit_test(exact_binding(), &mut intents);
        let ActionBindingEditorIntent::Test {
            identity, binding, ..
        } = intents.remove(0)
        else {
            panic!("expected explicit Test intent")
        };

        state.invalidate_pending_requests();

        assert_eq!(state.command, "window:show");
        assert_eq!(state.args, "unsaved argument draft");
        assert!(!state.matches_identity(&identity));
        assert!(!state.test_binding_matches(&identity, &binding));
        assert!(state.pending_test_binding.is_none());
    }

    #[test]
    fn disposal_retires_exact_pending_identities_and_clears_all_buffer_state() {
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope(), Some(&exact_binding()));
        state.query = "unassigned disposal query".into();
        state.command = "window:show".into();
        state.args = "unassigned disposal arguments".into();
        let mut intents = Vec::new();
        state.emit_search(&mut intents);
        state.emit_test(exact_binding(), &mut intents);
        let search_identity = state.pending_search_identity.clone().unwrap();
        let test_identity = state.pending_test_identity.clone().unwrap();
        let _ = crate::radial::acceptance_trace::take_authoring_provider_trace_test_events();

        state.dispose();

        assert_eq!(state, ActionBindingEditorState::default());
        assert!(!state.has_unassigned_text());
        assert!(state.acceptance_observation().is_none());
        assert!(!state.matches_identity(&search_identity));
        assert!(!state.matches_identity(&test_identity));
        let retired = crate::radial::acceptance_trace::take_authoring_provider_trace_test_events();
        assert_eq!(retired.len(), 2);
        assert_eq!(retired[0].edge, "retired");
        assert_eq!(retired[0].kind, "search");
        assert_eq!(retired[0].identity, search_identity);
        assert_eq!(retired[1].edge, "retired");
        assert_eq!(retired[1].kind, "test");
        assert_eq!(retired[1].identity, test_identity);
        state.dispose();
        assert!(
            crate::radial::acceptance_trace::take_authoring_provider_trace_test_events().is_empty()
        );
    }

    #[test]
    fn revisiting_the_same_scope_gets_a_new_editor_epoch() {
        let scope = scope();
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope.clone(), None);
        let first = state.identity().unwrap();
        state.reset_for(scope, None);
        let second = state.identity().unwrap();
        assert_ne!(first.editor_epoch, second.editor_epoch);
        assert!(!state.matches_identity(&first));
    }

    #[test]
    fn draft_revision_change_preserves_same_cell_buffers_but_retires_requests() {
        let binding = exact_binding();
        let mut first_scope = scope();
        let context = egui::Context::default();
        let mut state = ActionBindingEditorState::default();
        state.reset_for(first_scope.clone(), Some(&binding));
        state.query = "notes".into();
        state.command = "window:show".into();
        state.args = "unsaved argument draft".into();
        state.query_mode = QueryRunMode::ExecuteFirst;
        let mut intents = Vec::new();
        state.emit_test(binding.clone(), &mut intents);
        let ActionBindingEditorIntent::Test {
            identity: old_identity,
            binding: old_binding,
            ..
        } = intents.remove(0)
        else {
            panic!("expected Test intent")
        };

        first_scope.draft_generation = DraftGeneration(4);
        let _ = frame(
            &context,
            &mut state,
            first_scope.clone(),
            &binding,
            Vec::new(),
        );

        assert_eq!(state.query, "notes");
        assert_eq!(state.command, "window:show");
        assert_eq!(state.args, "unsaved argument draft");
        assert_eq!(state.query_mode, QueryRunMode::ExecuteFirst);
        assert!(state.matches_scope(&first_scope));
        assert!(!state.test_binding_matches(&old_identity, &old_binding));
        assert!(state.pending_search_due.is_some());
    }

    #[test]
    fn production_exact_command_and_argument_edits_invalidate_pending_test() {
        let binding = exact_binding();
        let scope = scope();
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut state = ActionBindingEditorState::default();
        state.reset_for(scope.clone(), Some(&binding));
        let mut intents = Vec::new();
        state.emit_test(binding.clone(), &mut intents);
        let ActionBindingEditorIntent::Test {
            identity: pending_identity,
            binding: pending_binding,
            ..
        } = intents.remove(0)
        else {
            panic!("expected Test intent")
        };

        let initial = frame(&context, &mut state, scope.clone(), &binding, Vec::new());
        let after_command = replace_text(
            &context,
            &mut state,
            &scope,
            &binding,
            &initial,
            "window:close",
            "window:show",
        );
        assert_eq!(state.command, "window:show");
        assert!(!state.test_binding_matches(&pending_identity, &pending_binding));

        let current_binding = ActionBinding::ExactCommand {
            command: state.command.clone(),
            args: Some(state.args.clone()),
        };
        let mut next_intents = Vec::new();
        state.emit_test(current_binding.clone(), &mut next_intents);
        let ActionBindingEditorIntent::Test {
            identity: pending_args_identity,
            binding: pending_args_binding,
            ..
        } = next_intents.remove(0)
        else {
            panic!("expected fresh Test intent before argument edit")
        };
        assert!(state.test_binding_matches(&pending_args_identity, &pending_args_binding));

        let after_args = replace_text(
            &context,
            &mut state,
            &scope,
            &binding,
            &after_command,
            "first argument",
            "second argument",
        );
        assert_eq!(state.args, "second argument");
        assert!(!state.test_binding_matches(&pending_args_identity, &pending_args_binding));
        assert!(after_args.platform_output.accesskit_update.is_some());
    }

    #[test]
    fn authoring_preview_uses_resolved_primary_action_without_execution_side_effects() {
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        install_window_target(&mut app);
        let query = "M4 Editor Fixture";
        let selected = test_action("window:close:44", query);
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: vec![selected.clone()],
            searched: None,
        }));

        let search_snapshot = app.plugins.search_snapshot(None, None).search(query);
        let ranked = app.search_read_only_outcome_with_plugin_snapshot(query, search_snapshot);
        let first = ranked.actions.first().expect("ranked first result");
        let (_, resolved_primary) = app
            .resolve_launcher_result_action(first, query)
            .expect("first window result resolves");
        let rows = app
            .authoring_catalog_for_ranked_actions(&ranked.actions, query)
            .rows()
            .to_vec();
        let primary_row = rows
            .iter()
            .find(|row| row.target_command == first.action && row.action_id == resolved_primary.id)
            .expect("catalog row for the primary action");
        let expected_preview = primary_row.display_label();
        assert_ne!(
            expected_preview,
            rows.first().expect("generic registry row").display_label(),
            "fixture distinguishes the resolved primary from registry row zero"
        );

        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::OpenLauncher,
        };
        let intent = install_properties_editor(&mut app, binding, query, false, false);
        route_editor_intent(&mut app, intent);
        wait_for_authoring_state(&mut app, |app| {
            app.radial_editor.lock().is_ok_and(|editor| {
                editor.properties_draft.as_ref().is_some_and(|draft| {
                    !matches!(&draft.action_editor.search_status, SearchStatus::Pending)
                })
            })
        });

        let editor = app.radial_editor.lock().expect("radial editor lock");
        let state = &editor.properties_draft.as_ref().unwrap().action_editor;
        assert_eq!(
            state.search_preview.as_deref(),
            Some(expected_preview.as_str())
        );
        assert_eq!(state.search_rows.len(), rows.len());
        assert!(app.test_activation_trace.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
        assert!(app.radial_preparations.is_empty());
        assert!(app.pending_universal_confirm.is_none());
    }

    #[test]
    fn rendered_result_rows_publish_the_exact_displayed_label_and_unclipped_bounds() {
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        let query = "M4 narrow pane target with a long descriptive identity";
        let action = test_action("exec:m4_rendered_label", query);
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: vec![action],
            searched: None,
        }));
        let snapshot = app.plugins.search_snapshot(None, None).search(query);
        let ranked = app.search_read_only_outcome_with_plugin_snapshot(query, snapshot);
        let rows = app
            .authoring_catalog_for_ranked_actions(&ranked.actions, query)
            .rows()
            .to_vec();
        let row = rows
            .iter()
            .find(|row| row.target_command == "exec:m4_rendered_label")
            .expect("fixture target row");
        let row_index = rows
            .iter()
            .position(|candidate| candidate.target_command == "exec:m4_rendered_label")
            .expect("fixture target row index");
        let row_identity = trace_picker_row_identity(row);
        let expected_label = row.display_label();
        let expected_digest =
            crate::radial::acceptance_trace::private_trace_text_digest(&expected_label);
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::OpenLauncher,
        };

        for surface in [
            BindingEditorSurface::Properties,
            BindingEditorSurface::Inspector,
        ] {
            let mut editor_scope = scope();
            editor_scope.surface = surface;
            let mut state = ActionBindingEditorState::default();
            state.reset_for(editor_scope.clone(), Some(&binding));
            state.search_rows = rows.clone();
            let mut expected_bounds = None;
            for monitor_origin in [
                egui::Pos2::ZERO,
                egui::pos2(188.0, 213.0),
                egui::pos2(-1440.0, 90.0),
            ] {
                let _output = frame_at_monitor_origin(
                    &context,
                    &mut state,
                    editor_scope.clone(),
                    &binding,
                    Vec::new(),
                    monitor_origin,
                );
                let events =
                    crate::radial::acceptance_trace::take_action_editor_control_test_events();
                let row_event = events.iter().find_map(|event| match event {
                    crate::radial::acceptance_trace::Event::DesignerActionEditorControl {
                        surface: emitted_surface,
                        control: "result_target",
                        control_index,
                        target_digest,
                        title_digest,
                        type_digest,
                        disambiguator_digest,
                        action_digest,
                        binding_digest,
                        displayed_text_digest,
                        full_left_px,
                        full_top_px,
                        full_right_px,
                        full_bottom_px,
                        fully_visible,
                        ..
                    } => Some((
                        *emitted_surface,
                        *control_index,
                        *target_digest,
                        *title_digest,
                        *type_digest,
                        *disambiguator_digest,
                        *action_digest,
                        *binding_digest,
                        *displayed_text_digest,
                        [*full_left_px, *full_top_px, *full_right_px, *full_bottom_px],
                        *fully_visible,
                    )),
                    _ => None,
                });
                let (
                    emitted_surface,
                    control_index,
                    target,
                    title,
                    target_type,
                    disambiguator,
                    action,
                    binding_digest,
                    digest,
                    bounds,
                    fully_visible,
                ) = row_event.expect("rendered row emits a production control event");
                assert_eq!(
                    emitted_surface,
                    match surface {
                        BindingEditorSurface::Properties => "properties",
                        BindingEditorSurface::Inspector => "inspector",
                    }
                );
                assert_eq!(control_index, Some(row_index));
                assert_eq!(
                    digest, expected_digest,
                    "digest binds the text actually shown"
                );
                let pin_event = events.iter().find_map(|event| match event {
                    crate::radial::acceptance_trace::Event::DesignerActionEditorControl {
                        surface: pin_surface,
                        control: "pin_result",
                        control_index: pin_index,
                        target_digest: pin_target,
                        title_digest: pin_title,
                        type_digest: pin_type,
                        disambiguator_digest: pin_disambiguator,
                        action_digest: pin_action,
                        binding_digest: pin_binding,
                        displayed_text_digest,
                        fully_visible: pin_fully_visible,
                        ..
                    } => Some((
                        *pin_surface,
                        *pin_index,
                        *pin_target,
                        *pin_title,
                        *pin_type,
                        *pin_disambiguator,
                        *pin_action,
                        *pin_binding,
                        *displayed_text_digest,
                        *pin_fully_visible,
                    )),
                    _ => None,
                });
                let (
                    pin_surface,
                    pin_index,
                    pin_target,
                    pin_title,
                    pin_type,
                    pin_disambiguator,
                    pin_action,
                    pin_binding,
                    pin_display_digest,
                    pin_fully_visible,
                ) = pin_event.expect("the production row also emits its Pin control");
                assert_eq!(pin_surface, emitted_surface);
                assert_eq!(pin_index, Some(row_index));
                assert_eq!(pin_target, target);
                assert_eq!(pin_title, title);
                assert_eq!(pin_type, target_type);
                assert_eq!(pin_disambiguator, disambiguator);
                assert_eq!(pin_action, action);
                assert_eq!(pin_binding, binding_digest);
                assert_eq!(
                    pin_display_digest, 0,
                    "Pin has no rendered row text of its own"
                );
                assert!(
                    pin_fully_visible,
                    "the Pin button is a separately visible control"
                );
                assert_eq!(target, row_identity.target);
                assert_eq!(title, row_identity.title);
                assert_eq!(target_type, row_identity.target_type);
                assert_eq!(disambiguator, row_identity.disambiguator);
                assert_eq!(action, row_identity.action);
                assert_eq!(binding_digest, row_identity.binding);
                assert!(bounds[2] > bounds[0] && bounds[3] > bounds[1]);
                assert!(
                    fully_visible,
                    "a client-local row remains visible at monitor origin {monitor_origin:?}"
                );
                if let Some(expected_bounds) = expected_bounds {
                    assert_eq!(
                        bounds, expected_bounds,
                        "monitor origin does not shift client-local bounds"
                    );
                } else {
                    expected_bounds = Some(bounds);
                }
            }

            let [left, top, right, bottom] = expected_bounds.expect("rendered row bounds");
            let partial_clip = egui::Rect::from_min_max(
                egui::Pos2::ZERO,
                egui::pos2((right - 1) as f32, bottom as f32),
            );
            let _output = frame_at_monitor_origin_with_clip(
                &context,
                &mut state,
                editor_scope.clone(),
                &binding,
                Vec::new(),
                egui::pos2(-1200.0, 160.0),
                Some(partial_clip),
            );
            let clipped_event =
                crate::radial::acceptance_trace::take_action_editor_control_test_events()
                    .into_iter()
                    .find_map(|event| match event {
                        crate::radial::acceptance_trace::Event::DesignerActionEditorControl {
                            control: "result_target",
                            displayed_text_digest,
                            full_left_px,
                            full_top_px,
                            full_right_px,
                            full_bottom_px,
                            fully_visible,
                            ..
                        } => Some((
                            displayed_text_digest,
                            [full_left_px, full_top_px, full_right_px, full_bottom_px],
                            fully_visible,
                        )),
                        _ => None,
                    })
                    .expect("partially clipped row still emits its rendered control");
            assert_eq!(clipped_event.0, expected_digest);
            assert_eq!(clipped_event.1, [left, top, right, bottom]);
            assert!(
                !clipped_event.2,
                "partially clipped row is not fully visible"
            );

            state.selected_binding = Some(row.binding.clone().expect("assignable fixture row"));
            let _output =
                frame_in_narrow_panel(&context, &mut state, editor_scope, &binding, 300.0);
            let narrow_events =
                crate::radial::acceptance_trace::take_action_editor_control_test_events();
            let narrow_row = narrow_events
                .iter()
                .find_map(|event| match event {
                    crate::radial::acceptance_trace::Event::DesignerActionEditorControl {
                        surface: emitted_surface,
                        control: "result_target",
                        control_index,
                        target_digest,
                        title_digest,
                        type_digest,
                        disambiguator_digest,
                        action_digest,
                        binding_digest,
                        displayed_text_digest,
                        full_left_px,
                        full_top_px,
                        full_right_px,
                        full_bottom_px,
                        fully_visible,
                        selected,
                        ..
                    } => Some((
                        *emitted_surface,
                        *control_index,
                        *target_digest,
                        *title_digest,
                        *type_digest,
                        *disambiguator_digest,
                        *action_digest,
                        *binding_digest,
                        *displayed_text_digest,
                        [*full_left_px, *full_top_px, *full_right_px, *full_bottom_px],
                        *fully_visible,
                        *selected,
                    )),
                    _ => None,
                })
                .expect("narrow Inspector still publishes the result row");
            assert_eq!(
                narrow_row.0,
                match surface {
                    BindingEditorSurface::Properties => "properties",
                    BindingEditorSurface::Inspector => "inspector",
                }
            );
            assert_eq!(narrow_row.1, Some(row_index));
            assert_eq!(narrow_row.2, row_identity.target);
            assert_eq!(narrow_row.3, row_identity.title);
            assert_eq!(narrow_row.4, row_identity.target_type);
            assert_eq!(narrow_row.5, row_identity.disambiguator);
            assert_eq!(narrow_row.6, row_identity.action);
            assert_eq!(narrow_row.7, row_identity.binding);
            assert_eq!(narrow_row.8, expected_digest);
            assert!(
                narrow_row.9[2] <= 300,
                "wrapped label stays in the narrow pane: {:?}",
                narrow_row.9
            );
            assert!(
                narrow_row.9[3] - narrow_row.9[1] > 24,
                "long target/action identity wraps to multiple lines"
            );
            assert!(narrow_row.10, "the complete wrapped label is readable");
            assert!(
                narrow_row.11,
                "row selection state is preserved while wrapping"
            );

            for (control_name, expected_target) in [
                ("pin_result", row_identity.target),
                ("test_result", row_identity.target),
            ] {
                let control = narrow_events
                    .iter()
                    .find_map(|event| match event {
                        crate::radial::acceptance_trace::Event::DesignerActionEditorControl {
                            control,
                            control_index,
                            target_digest,
                            full_right_px,
                            fully_visible,
                            ..
                        } if *control == control_name => Some((
                            *control_index,
                            *target_digest,
                            *full_right_px,
                            *fully_visible,
                        )),
                        _ => None,
                    })
                    .expect("row action control is emitted beneath its label");
                assert_eq!(control.0, Some(row_index));
                assert_eq!(control.1, expected_target);
                assert!(control.2 <= 300, "action control stays in the narrow pane");
                assert!(control.3, "action control remains visible");
            }
        }
    }

    #[test]
    fn live_ranked_editor_search_scrolls_and_assigns_rows_beyond_fifty() {
        crate::radial::acceptance_trace::reset_action_editor_scroll_test_state();
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut app = crate::gui::actions::tests::new_app(&context);
        app.test_skip_history_persistence = true;
        let history_before = serde_json::to_vec(
            &crate::history::with_history(Clone::clone).expect("history snapshot"),
        )
        .expect("serialize history");
        let usage_before = app.usage.clone();
        let activation_before = app.test_activation_trace.clone();

        let mut custom_actions = vec![
            test_action("qmarker-alpha", "QMarker Alpha"),
            test_action("qmarker-beta", "QMarker Beta"),
        ];
        custom_actions.extend((0..64).map(|index| {
            test_action(
                &format!("radial_acceptance_harmless_{index:03}"),
                &format!("Radial Acceptance Harmless Action {index:03}"),
            )
        }));
        for (index, custom_action) in custom_actions.iter_mut().enumerate().skip(34) {
            custom_action.label = format!("Radial Acceptance Secondary Action {:03}", index - 2);
        }
        app.custom_len = custom_actions.len();
        app.actions = std::sync::Arc::new(custom_actions);
        app.update_action_cache();
        app.query_results_layout.enabled = true;
        app.query_results_layout.respect_plugin_capability = true;

        let query = "app Radial Acceptance Harmless Action";
        app.query = query.into();
        app.search();
        let ranked = app.search_read_only_outcome(query);
        assert_eq!(
            ranked.state,
            crate::gui::search::LauncherSearchState::Results
        );
        assert_eq!(ranked.actions.len(), 32);
        let catalog = app.authoring_catalog_for_ranked_actions(&ranked.actions, query);
        let rows = catalog.rows().to_vec();
        assert!(rows.len() > 50);
        let target_row_index = rows
            .iter()
            .position(|row| {
                row.custom_action_index == Some(32)
                    && row.target_command == "radial_acceptance_harmless_030"
                    && row.action_id == crate::universal_actions::action_ids::RESULT_EXECUTE
            })
            .expect("the actual app-ranked target has an Execute row");
        assert!(target_row_index >= 50, "target row was {target_row_index}");
        let expected_binding = rows[target_row_index]
            .assignment()
            .expect("ranked custom target can be pinned");

        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::OpenLauncher,
        };
        let mut editor_scope = scope();
        editor_scope.surface = BindingEditorSurface::Inspector;
        let mut state = ActionBindingEditorState::default();
        state.reset_for(editor_scope.clone(), Some(&binding));
        let mut search_intents = Vec::new();
        state.emit_search(&mut search_intents);
        let search_identity = match search_intents.into_iter().next().expect("search intent") {
            ActionBindingEditorIntent::Search {
                identity,
                query: sent_query,
            } => {
                assert_eq!(sent_query, query);
                identity
            }
            other => panic!("expected editor Search intent, got {other:?}"),
        };
        assert!(state.complete_search(ActionSearchCompletion {
            identity: search_identity,
            state: ActionSearchCompletionState::Results,
            rows,
            preview: Some("first ranked result".into()),
            preview_unavailable: None,
        }));
        assert_eq!(state.search_rows.len(), catalog.rows().len());

        let empty_action_catalog = action_snapshot(Vec::new());
        let _ = crate::radial::acceptance_trace::take_action_editor_control_test_events();
        let _ = render_editor_with_catalog(
            &context,
            &mut state,
            editor_scope.clone(),
            &binding,
            &empty_action_catalog,
            Vec::new(),
        );
        let initial_controls =
            crate::radial::acceptance_trace::take_action_editor_control_test_events();
        let scroll_origin = initial_controls
            .iter()
            .find_map(|event| match event {
                crate::radial::acceptance_trace::Event::DesignerActionEditorControl {
                    control: "result_target",
                    control_index: Some(0),
                    full_left_px,
                    full_top_px,
                    full_right_px,
                    full_bottom_px,
                    ..
                } => Some(egui::pos2(
                    (*full_left_px + *full_right_px) as f32 / 2.0,
                    (*full_top_px + *full_bottom_px) as f32 / 2.0,
                )),
                _ => None,
            })
            .expect("the first live-ranked editor row is rendered");

        let mut clicked_pin_rect = None;
        let mut maximum_offset_y_milli = 0i64;
        for (direction, steps) in [(-1.0, 18), (1.0, 36)] {
            for _ in 0..steps {
                let (_, intents) = render_editor_with_catalog(
                    &context,
                    &mut state,
                    editor_scope.clone(),
                    &binding,
                    &empty_action_catalog,
                    vec![
                        egui::Event::PointerMoved(scroll_origin),
                        egui::Event::Scroll(egui::vec2(0.0, direction * 700.0)),
                    ],
                );
                assert!(intents.is_empty());
                for event in
                    crate::radial::acceptance_trace::take_action_editor_scroll_test_events()
                {
                    if let crate::radial::acceptance_trace::Event::DesignerActionEditorScroll {
                        offset_y_milli,
                        ..
                    } = event
                    {
                        maximum_offset_y_milli = maximum_offset_y_milli.max(offset_y_milli);
                    }
                }
                let controls =
                    crate::radial::acceptance_trace::take_action_editor_control_test_events();
                clicked_pin_rect = controls.into_iter().find_map(|event| match event {
                    crate::radial::acceptance_trace::Event::DesignerActionEditorControl {
                        control: "pin_result",
                        control_index: Some(index),
                        full_left_px,
                        full_top_px,
                        full_right_px,
                        full_bottom_px,
                        enabled: true,
                        fully_visible: true,
                        ..
                    } if index == target_row_index => {
                        Some([full_left_px, full_top_px, full_right_px, full_bottom_px])
                    }
                    _ => None,
                });
                if clicked_pin_rect.is_some() {
                    break;
                }
            }
            if clicked_pin_rect.is_some() {
                break;
            }
        }
        assert!(maximum_offset_y_milli > 0, "the live editor list scrolled");
        let pin_rect =
            clicked_pin_rect.expect("the beyond-50 Pin control is reachable by scrolling");
        let pin_position = egui::pos2(
            (pin_rect[0] + pin_rect[2]) as f32 / 2.0,
            (pin_rect[1] + pin_rect[3]) as f32 / 2.0,
        );
        let mut pin_intents = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(pin_position)],
            vec![pointer_button(pin_position, true)],
            vec![pointer_button(pin_position, false)],
        ] {
            let (_, intents) = render_editor_with_catalog(
                &context,
                &mut state,
                editor_scope.clone(),
                &binding,
                &empty_action_catalog,
                events,
            );
            pin_intents.extend(intents);
            let _ = crate::radial::acceptance_trace::take_action_editor_control_test_events();
        }
        let assigned_binding = pin_intents.into_iter().find_map(|intent| match intent {
            ActionBindingEditorIntent::Pin { binding } => Some(binding),
            _ => None,
        });
        assert_eq!(assigned_binding, Some(expected_binding.clone()));
        assert_eq!(state.selected_binding(), Some(expected_binding.clone()));
        assert_eq!(state.active_binding, Some(binding.clone()));
        assert!(
            state.has_unassigned_text(),
            "a selected Pin candidate is pending until its document commit succeeds"
        );
        state
            .finish_assignment(
                expected_binding.clone(),
                BindingAssignmentKind::Pinned,
                Ok::<(), ()>(()),
            )
            .expect("fixture assignment commit succeeds");
        assert_eq!(state.query, query, "Pin keeps the result search visible");
        assert!(
            !state.has_unassigned_text(),
            "the retained result search is not a pending action edit"
        );
        let pinned_output = render_editor_with_catalog(
            &context,
            &mut state,
            editor_scope.clone(),
            &expected_binding,
            &empty_action_catalog,
            Vec::new(),
        )
        .0;
        let replacement_query = "a new authored query after pin";
        let _ = replace_text(
            &context,
            &mut state,
            &editor_scope,
            &expected_binding,
            &pinned_output,
            query,
            replacement_query,
        );
        assert!(state.has_unassigned_text());
        assert_eq!(
            state.binding_for_explicit_apply(),
            Some(ActionBinding::LauncherQuery {
                query: replacement_query.into(),
                mode: QueryRunMode::OpenLauncher,
            })
        );
        assert!(app.test_activation_trace.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
        assert_eq!(app.usage, usage_before);
        assert_eq!(app.test_activation_trace, activation_before);
        assert_eq!(
            serde_json::to_vec(
                &crate::history::with_history(Clone::clone).expect("history snapshot"),
            )
            .expect("serialize history"),
            history_before,
        );
    }

    #[test]
    fn nested_inspector_scrollbar_has_an_independent_hittable_gutter() {
        crate::radial::acceptance_trace::reset_action_editor_scroll_test_state();
        let _ = crate::radial::acceptance_trace::take_action_editor_control_test_events();
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut app = crate::gui::actions::tests::new_app(&context);
        let query = "M4 compact nested Inspector scrollbar fixture";
        let actions = (0..72)
            .map(|index| {
                test_action(
                    &format!("exec:m4_nested_scroll_{index:02}"),
                    &format!("{query} action {index:02}"),
                )
            })
            .collect::<Vec<_>>();
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: actions,
            searched: None,
        }));
        let ranked = app.search_read_only_outcome_with_plugin_snapshot(
            query,
            app.plugins.search_snapshot(None, None).search(query),
        );
        let rows = app
            .authoring_catalog_for_ranked_actions(&ranked.actions, query)
            .rows()
            .to_vec();
        assert!(rows.len() > 50, "the nested list must exceed 50 results");

        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::OpenLauncher,
        };
        let mut inspector_scope = scope();
        inspector_scope.surface = BindingEditorSurface::Inspector;
        let mut state = ActionBindingEditorState::default();
        state.reset_for(inspector_scope.clone(), Some(&binding));
        state.search_rows = rows.clone();

        let mut time = 0.0;
        let mut latest_scroll = None;
        let mut inspector_controls = Vec::new();
        let mut initial_frame = None;
        for _ in 0..12 {
            let rendered = nested_inspector_frame(
                &context,
                &mut state,
                inspector_scope.clone(),
                &binding,
                time,
                Vec::new(),
            );
            time += 1.0 / 60.0;
            if let Some(geometry) = action_editor_scroll_geometry(
                &crate::radial::acceptance_trace::take_action_editor_scroll_test_events(),
            ) {
                latest_scroll = Some(geometry);
            }
            inspector_controls
                .extend(crate::radial::acceptance_trace::take_action_editor_control_test_events());
            if latest_scroll.is_some() && rendered.outer_bar_rects.is_some() {
                initial_frame = Some(rendered);
                break;
            }
        }

        let initial_frame = initial_frame.expect("both nested scrollbar responses are rendered");
        let geometry = latest_scroll.expect("the Inspector result scroll remains traced");
        assert!(
            initial_frame
                .output
                .platform_output
                .accesskit_update
                .is_some()
        );
        assert!(initial_frame.outer_content_height > initial_frame.outer_inner_height);
        assert_eq!(geometry.client_size, [900, 650]);
        assert_eq!(geometry.offset_y_milli, 0);
        assert_eq!(
            geometry.inner[3] - geometry.inner[1],
            64,
            "result viewport geometry: {:?}",
            geometry.inner
        );
        let (outer_bar_rect, outer_hit_rect) = initial_frame
            .outer_bar_rects
            .expect("outer scrollbar response");
        assert!(outer_bar_rect.is_positive());
        assert!(outer_hit_rect.is_positive());

        let pixels_per_point = geometry.pixels_per_point_milli as f32 / 1_000.0;
        let rect_from_pixels = |bounds: [i32; 4]| {
            egui::Rect::from_min_max(
                egui::pos2(
                    bounds[0] as f32 / pixels_per_point,
                    bounds[1] as f32 / pixels_per_point,
                ),
                egui::pos2(
                    bounds[2] as f32 / pixels_per_point,
                    bounds[3] as f32 / pixels_per_point,
                ),
            )
        };
        for inner_bounds in [
            geometry.inner,
            geometry.track,
            geometry.track_visible,
            geometry.thumb,
            geometry.thumb_visible,
            geometry.painted_thumb,
            geometry.painted_thumb_visible,
        ] {
            let inner_rect = rect_from_pixels(inner_bounds);
            assert!(
                !inner_rect.intersects(outer_bar_rect),
                "inner painted/track geometry intersects the outer bar: {inner_rect:?} vs {outer_bar_rect:?}"
            );
            assert!(
                !inner_rect.intersects(outer_hit_rect),
                "inner painted/track geometry intersects the outer hit region: {inner_rect:?} vs {outer_hit_rect:?}"
            );
        }

        let thumb_rect = rect_from_pixels(geometry.thumb);
        let start = thumb_rect.center();
        let activation_excursion = start + egui::vec2(0.0, 8.0);
        let release = start + egui::vec2(0.0, 1.0);
        let visible_track = rect_from_pixels(geometry.track_visible);
        assert!(visible_track.contains(start));
        assert!(visible_track.contains(activation_excursion));
        assert!(visible_track.contains(release));
        for point in [start, activation_excursion, release] {
            assert!(
                !outer_hit_rect.contains(point),
                "the planned inner thumb drag point is owned by the outer scrollbar: {point:?}"
            );
        }

        let expected_label_digest =
            crate::radial::acceptance_trace::private_trace_text_digest(&rows[0].display_label());
        assert!(
            inspector_controls.iter().any(|event| matches!(
                event,
                crate::radial::acceptance_trace::Event::DesignerActionEditorControl {
                    control: "result_target",
                    control_index: Some(0),
                    displayed_text_digest,
                    fully_visible: true,
                    ..
                } if *displayed_text_digest == expected_label_digest
            )),
            "the compact Inspector keeps the complete first result label visible"
        );
        assert!(
            inspector_controls.iter().any(|event| matches!(
                event,
                crate::radial::acceptance_trace::Event::DesignerActionEditorControl {
                    control: "pin_result",
                    control_index: Some(0),
                    enabled: true,
                    fully_visible: true,
                    ..
                }
            )),
            "the compact Inspector keeps the first result's Pin button reachable"
        );

        let outer_offset_before = initial_frame.outer_offset_y;
        let mut after_drag = geometry;
        for events in [
            vec![egui::Event::PointerMoved(start)],
            vec![pointer_button(start, true)],
            vec![egui::Event::PointerMoved(activation_excursion)],
            vec![egui::Event::PointerMoved(release)],
            vec![pointer_button(release, false)],
        ] {
            let rendered = nested_inspector_frame(
                &context,
                &mut state,
                inspector_scope.clone(),
                &binding,
                time,
                events,
            );
            time += 1.0 / 60.0;
            assert_eq!(rendered.outer_offset_y, outer_offset_before);
            if let Some(geometry) = action_editor_scroll_geometry(
                &crate::radial::acceptance_trace::take_action_editor_scroll_test_events(),
            ) {
                after_drag = geometry;
            }
            let _ = crate::radial::acceptance_trace::take_action_editor_control_test_events();
        }
        assert!(
            after_drag.offset_y_milli > geometry.offset_y_milli,
            "dragging the inner thumb down advances only the inner result list"
        );

        let mut properties_scope = scope();
        properties_scope.surface = BindingEditorSurface::Properties;
        let mut properties_state = ActionBindingEditorState::default();
        properties_state.reset_for(properties_scope.clone(), Some(&binding));
        properties_state.search_rows = rows;
        let properties_context = egui::Context::default();
        crate::radial::acceptance_trace::reset_action_editor_scroll_test_state();
        let mut properties_geometry = None;
        for _ in 0..6 {
            let _ = frame(
                &properties_context,
                &mut properties_state,
                properties_scope.clone(),
                &binding,
                Vec::new(),
            );
            if let Some(geometry) = action_editor_scroll_geometry(
                &crate::radial::acceptance_trace::take_action_editor_scroll_test_events(),
            ) {
                properties_geometry = Some(geometry);
                break;
            }
        }
        let properties_geometry =
            properties_geometry.expect("standalone Properties keeps its result scrollbar");
        assert_eq!(properties_geometry.client_size, [900, 520]);
        assert!(
            properties_geometry.track[2] > 885,
            "standalone Properties keeps its full-width scrollbar region"
        );
    }

    #[test]
    fn result_scroll_trace_publishes_identity_and_measured_thumb_geometry() {
        crate::radial::acceptance_trace::reset_action_editor_scroll_test_state();
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        let query = "M4 private scrollbar query fixture";
        let actions = (0..128)
            .map(|index| {
                test_action(
                    &format!("exec:m4_scroll_{index:02}"),
                    &format!("{query} action {index:02}"),
                )
            })
            .collect::<Vec<_>>();
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: actions.clone(),
            searched: None,
        }));
        let ranked = app.search_read_only_outcome_with_plugin_snapshot(
            query,
            app.plugins.search_snapshot(None, None).search(query),
        );
        let rows = app
            .authoring_catalog_for_ranked_actions(&ranked.actions, query)
            .rows()
            .to_vec();
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::OpenLauncher,
        };
        let editor_scope = scope();
        let mut state = ActionBindingEditorState::default();
        state.reset_for(editor_scope.clone(), Some(&binding));
        state.search_rows = rows;
        let identity = state.identity().expect("scoped action editor identity");

        let mut events = Vec::new();
        // The first ScrollArea frame can reserve its scrollbar without
        // registering the interaction response until the visibility
        // animation has advanced. Keep rendering deterministic frames until
        // the owner publishes the actual current-frame track geometry.
        for _ in 0..4 {
            let _ = frame(
                &context,
                &mut state,
                editor_scope.clone(),
                &binding,
                Vec::new(),
            );
            events.extend(crate::radial::acceptance_trace::take_action_editor_scroll_test_events());
            if events.iter().any(|event| {
                matches!(
                    event,
                    crate::radial::acceptance_trace::Event::DesignerActionEditorScroll { .. }
                )
            }) {
                break;
            }
        }
        let scroll = events.iter().find_map(|event| match event {
            crate::radial::acceptance_trace::Event::DesignerActionEditorScroll {
                surface,
                editor_session_id,
                stable_target_digest,
                editor_epoch,
                query_digest,
                scroll_id,
                frame_nr,
                content_height_milli,
                inner_height_milli,
                pixels_per_point_milli,
                handle_min_length_milli,
                inner_left_px,
                inner_top_px,
                inner_right_px,
                inner_bottom_px,
                track_left_px,
                track_top_px,
                track_right_px,
                track_bottom_px,
                thumb_left_px,
                thumb_top_px,
                thumb_right_px,
                thumb_bottom_px,
                thumb_visible_left_px,
                thumb_visible_top_px,
                thumb_visible_right_px,
                thumb_visible_bottom_px,
                painted_thumb_left_px,
                painted_thumb_top_px,
                painted_thumb_right_px,
                painted_thumb_bottom_px,
                painted_thumb_visible_left_px,
                painted_thumb_visible_top_px,
                painted_thumb_visible_right_px,
                painted_thumb_visible_bottom_px,
                client_width_px,
                client_height_px,
                ..
            } => Some((
                *surface,
                *editor_session_id,
                *stable_target_digest,
                *editor_epoch,
                *query_digest,
                *scroll_id,
                *frame_nr,
                *content_height_milli,
                *inner_height_milli,
                *pixels_per_point_milli,
                *handle_min_length_milli,
                [
                    *inner_left_px,
                    *inner_top_px,
                    *inner_right_px,
                    *inner_bottom_px,
                ],
                [
                    *track_left_px,
                    *track_top_px,
                    *track_right_px,
                    *track_bottom_px,
                ],
                [
                    *thumb_left_px,
                    *thumb_top_px,
                    *thumb_right_px,
                    *thumb_bottom_px,
                ],
                [
                    *thumb_visible_left_px,
                    *thumb_visible_top_px,
                    *thumb_visible_right_px,
                    *thumb_visible_bottom_px,
                ],
                [
                    *painted_thumb_left_px,
                    *painted_thumb_top_px,
                    *painted_thumb_right_px,
                    *painted_thumb_bottom_px,
                ],
                [
                    *painted_thumb_visible_left_px,
                    *painted_thumb_visible_top_px,
                    *painted_thumb_visible_right_px,
                    *painted_thumb_visible_bottom_px,
                ],
                [*client_width_px, *client_height_px],
            )),
            _ => None,
        });
        let Some((
            surface,
            session_id,
            stable_target,
            editor_epoch,
            query_digest,
            scroll_id,
            frame_nr,
            content_height,
            inner_height,
            pixels_per_point,
            handle_min_length,
            inner,
            track,
            thumb,
            visible_thumb,
            painted_thumb,
            visible_painted_thumb,
            client,
        )) = scroll
        else {
            panic!("overflowing result list must publish a scrollbar geometry receipt");
        };
        assert_eq!(surface, "properties");
        assert_eq!(session_id, identity.scope.editor_session.0);
        let mut expected_target_hasher = std::collections::hash_map::DefaultHasher::new();
        identity.scope.target.hash(&mut expected_target_hasher);
        assert_eq!(stable_target, expected_target_hasher.finish());
        assert_eq!(editor_epoch, identity.editor_epoch);
        assert_eq!(
            query_digest,
            crate::radial::acceptance_trace::private_trace_text_digest(query)
        );
        assert_ne!(scroll_id, 0);
        assert!(frame_nr > 0);
        assert!(content_height > inner_height && inner_height > 0);
        assert!(pixels_per_point > 0);
        assert!(handle_min_length >= 12_000);
        assert!(inner[2] > inner[0] && inner[3] > inner[1]);
        assert!(track[2] > track[0] && track[3] > track[1]);
        assert!(thumb[2] > thumb[0] && thumb[3] > thumb[1]);
        assert!(visible_thumb[2] > visible_thumb[0] && visible_thumb[3] > visible_thumb[1]);
        assert!(painted_thumb[2] > painted_thumb[0] && painted_thumb[3] > painted_thumb[1]);
        assert!(visible_painted_thumb[2] > visible_painted_thumb[0]);
        assert!(visible_painted_thumb[3] > visible_painted_thumb[1]);
        assert!(track[0] <= thumb[0] && track[2] >= thumb[2]);
        assert!(track[1] <= thumb[1] && track[3] >= thumb[3]);
        let raw_height = thumb[3] - thumb[1];
        let painted_height = painted_thumb[3] - painted_thumb[1];
        let min_painted_height =
            ((handle_min_length as i64 * pixels_per_point as i64) + 500_000) / 1_000_000;
        assert!(
            i64::from(raw_height) < min_painted_height,
            "dense result rows exercise egui's minimum painted handle"
        );
        assert!(painted_height >= min_painted_height as i32);
        assert!(
            ((painted_thumb[1] + painted_thumb[3]) - (thumb[1] + thumb[3])).abs() <= 2,
            "minimum-size painted handle stays centered on the raw drag mapping"
        );
        let raw_center = [(thumb[0] + thumb[2]) / 2, (thumb[1] + thumb[3]) / 2];
        assert!(raw_center[0] >= visible_thumb[0] && raw_center[0] <= visible_thumb[2]);
        assert!(raw_center[1] >= visible_thumb[1] && raw_center[1] <= visible_thumb[3]);
        assert!(raw_center[0] >= visible_painted_thumb[0]);
        assert!(raw_center[0] <= visible_painted_thumb[2]);
        assert!(raw_center[1] >= visible_painted_thumb[1]);
        assert!(raw_center[1] <= visible_painted_thumb[3]);
        assert_eq!(client, [900, 520]);
        let rendered = format!("{events:?}");
        assert!(
            !rendered.contains(query),
            "scroll diagnostics must not contain raw query text"
        );
    }

    #[test]
    fn authoring_execute_first_test_dispatches_current_first_result_once() {
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        app.test_skip_history_persistence = true;
        let query = "M4 first result query";
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        let intent = install_properties_editor(&mut app, binding.clone(), query, false, true);
        let primary = crate::actions::Action {
            label: query.into(),
            desc: "M4 explicit Test primary fixture".into(),
            action: "exec:m4_first_result".into(),
            args: Some("separate first-result args".into()),
        };
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: vec![primary.clone()],
            searched: None,
        }));

        let received = Arc::new(Mutex::new(Vec::<crate::actions::Action>::new()));
        let callback_received = Arc::clone(&received);
        crate::gui::set_execute_action_hook(Some(Box::new(move |action| {
            callback_received.lock().unwrap().push(action.clone());
            Ok(())
        })));
        route_editor_intent(&mut app, intent);
        wait_for_authoring_state(&mut app, |app| {
            app.radial_editor.lock().is_ok_and(|editor| {
                editor.properties_draft.as_ref().is_some_and(|draft| {
                    draft.action_editor.test_status.as_deref() != Some("Test pending")
                })
            })
        });
        crate::gui::set_execute_action_hook(None);

        assert_eq!(app.test_activation_trace.len(), 1);
        assert_eq!(app.test_activation_trace[0].0, primary);
        assert_eq!(
            app.test_activation_trace[0].1,
            crate::commands::ActivationSource::Click
        );
        let received = received.lock().unwrap();
        assert_eq!(received.as_slice(), &[primary]);
        assert_eq!(
            received[0].args.as_deref(),
            Some("separate first-result args")
        );
        assert_eq!(app.test_recorded_history_queries, [query]);
        assert_eq!(app.usage.get("exec:m4_first_result"), Some(&1));
        assert!(app.radial_preparations.is_empty());
        assert!(app.radial_current_preparation.is_none());
        assert!(app.pending_universal_confirm.is_none());
    }

    #[test]
    fn stable_authoring_no_result_opens_original_query_without_execution() {
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        app.test_skip_history_persistence = true;
        let query = "m4-stable-no-result-query-49017";
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        let intent = install_properties_editor(&mut app, binding.clone(), query, false, true);
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: Vec::new(),
            searched: None,
        }));
        route_editor_intent(&mut app, intent);
        wait_for_authoring_state(&mut app, |app| {
            app.radial_editor.lock().is_ok_and(|editor| {
                editor.properties_draft.as_ref().is_some_and(|draft| {
                    draft.action_editor.test_status.as_deref() != Some("Test pending")
                })
            })
        });

        assert_eq!(app.query, query);
        assert!(app.visible_flag.load(std::sync::atomic::Ordering::SeqCst));
        assert!(app.test_activation_trace.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
        assert!(app.radial_preparations.is_empty());
    }

    #[test]
    fn authoring_test_never_skips_an_unavailable_ranked_first_result() {
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        app.test_skip_history_persistence = true;
        app.usage_weight = 1.0;
        let query = "m4-unavailable-first-result-78142";
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        app.query = "M4 existing root query".into();
        app.visible_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let unavailable = test_action("window:close:999", "first but unavailable");
        let second = test_action("exec:m4_must_not_skip_to_second", "second available");
        app.usage.insert(unavailable.action.clone(), 1_000);
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: vec![unavailable.clone(), second.clone()],
            searched: None,
        }));
        let intent = install_properties_editor(&mut app, binding, query, false, true);

        route_editor_intent(&mut app, intent);
        let event = receive_authoring_search_result(&mut app);
        let crate::gui::WatchEvent::RadialAuthoringSearchReady { request, result } = event else {
            panic!("the provider should return a stable ranked result")
        };
        let ranked = app.search_read_only_outcome_with_plugin_snapshot(query, result.clone());
        assert_eq!(ranked.actions.first(), Some(&unavailable));
        assert_eq!(ranked.actions.get(1), Some(&second));
        app.complete_authoring_provider_search(request, result);

        assert_eq!(
            app.query, query,
            "an unavailable first result opens the saved query"
        );
        assert!(
            app.test_activation_trace.is_empty(),
            "opening the original query must not execute a leaf action"
        );
        assert!(app.visible_flag.load(std::sync::atomic::Ordering::SeqCst));
        assert!(app.test_recorded_history_queries.is_empty());
        assert_eq!(app.usage.get(&unavailable.action), Some(&1_000));
        assert!(!app.usage.contains_key(&second.action));
        assert!(app.radial_preparations.is_empty());
        let editor = app.radial_editor.lock().expect("radial editor lock");
        assert!(
            editor
                .properties_draft
                .as_ref()
                .and_then(|draft| draft.action_editor.test_status.as_deref())
                .is_some_and(|status| status.starts_with("Test unavailable:"))
        );
    }

    #[test]
    fn provider_failure_test_falls_back_to_the_original_query() {
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        app.test_skip_history_persistence = true;
        let query = "m4-provider-failure-query-61803";
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        app.query = "M4 root before provider failure".into();
        app.visible_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);
        app.plugins.register(Box::new(PanickingSearchPlugin));
        let intent = install_properties_editor(&mut app, binding, query, false, true);

        route_editor_intent(&mut app, intent);
        wait_for_authoring_state(&mut app, |app| {
            app.radial_editor.lock().is_ok_and(|editor| {
                editor.properties_draft.as_ref().is_some_and(|draft| {
                    draft.action_editor.test_status.as_deref() != Some("Test pending")
                })
            })
        });

        assert_eq!(app.query, query);
        assert!(app.visible_flag.load(std::sync::atomic::Ordering::SeqCst));
        assert!(app.test_activation_trace.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
        assert!(app.radial_preparations.is_empty());
        let editor = app.radial_editor.lock().expect("radial editor lock");
        assert!(
            editor
                .properties_draft
                .as_ref()
                .and_then(|draft| draft.action_editor.test_status.as_deref())
                .is_some_and(|status| status.starts_with("Test unavailable:"))
        );
    }

    #[test]
    fn provider_revision_change_retries_through_stable_no_result_completion() {
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        app.test_skip_history_persistence = true;
        let query = "m4-revision-change-no-fallback-53017";
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        app.query = "M4 root before revision retry".into();
        app.visible_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        app.plugins.register(Box::new(BlockedRevisionSearchPlugin {
            started: started_tx,
            release: Mutex::new(release_rx),
            calls: std::sync::atomic::AtomicUsize::new(0),
        }));
        let intent = install_properties_editor(&mut app, binding, query, true, true);
        route_editor_intent(&mut app, intent);
        assert_eq!(
            started_rx.recv_timeout(std::time::Duration::from_secs(3)),
            Ok(1)
        );

        app.plugins
            .notify_search_update_for_test("m4_authoring_revision_retry_fixture");
        release_tx.send(()).expect("release first provider attempt");
        let event = receive_authoring_search_result(&mut app);
        let crate::gui::WatchEvent::RadialAuthoringSearchReady { request, result } = event else {
            panic!("revision change should be observed on a real search completion")
        };
        app.complete_authoring_provider_search(request, result);

        let queued_retry = app
            .radial_editor
            .lock()
            .expect("radial editor lock")
            .intent_bridge()
            .pending_authoring_searches
            .lock()
            .expect("authoring request queue")
            .iter()
            .any(|(request, _)| request.retry_attempt == 1);
        assert!(
            queued_retry,
            "a changed provider revision must retry the live Test"
        );
        assert_eq!(
            app.query, "M4 root before revision retry",
            "Pending must not fall back to the saved query"
        );
        assert!(!app.visible_flag.load(std::sync::atomic::Ordering::SeqCst));
        assert!(app.test_activation_trace.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
        assert!(app.radial_preparations.is_empty());

        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        let mut second_started = false;
        while Instant::now() < deadline && !second_started {
            app.process_watch_events();
            app.start_next_authoring_provider_search();
            second_started = started_rx.try_recv().is_ok();
            if !second_started {
                std::thread::yield_now();
            }
        }
        assert!(
            second_started,
            "the revision change must restart the latest request"
        );
        assert!(app.radial_provider_search_capacity.is_occupied());
        assert_eq!(
            app.query, "M4 root before revision retry",
            "the retry remains pending before its result arrives"
        );
        assert!(!app.visible_flag.load(std::sync::atomic::Ordering::SeqCst));
        assert!(app.test_activation_trace.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());

        release_tx
            .send(())
            .expect("release retried provider attempt");
        wait_for_authoring_state(&mut app, |app| {
            app.radial_editor.lock().is_ok_and(|editor| {
                editor.properties_draft.as_ref().is_some_and(|draft| {
                    draft.action_editor.test_status.as_deref() != Some("Test pending")
                })
            })
        });
        assert_eq!(
            app.query, query,
            "only the stable no-result retry may fall back"
        );
        assert!(app.visible_flag.load(std::sync::atomic::Ordering::SeqCst));
        assert!(app.test_activation_trace.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
        assert!(app.radial_preparations.is_empty());
        let editor = app.radial_editor.lock().expect("radial editor lock");
        assert!(
            editor
                .properties_draft
                .as_ref()
                .and_then(|draft| draft.action_editor.test_status.as_deref())
                .is_some_and(|status| status.starts_with("Test unavailable:"))
        );
    }

    #[test]
    fn authoring_exact_command_confirmation_is_correlated_and_preserves_args() {
        let context = egui::Context::default();
        let binding = ActionBinding::ExactCommand {
            command: "window:close:44".into(),
            args: Some("exact unsaved confirmation args".into()),
        };
        let history_query = "current exact editor text";

        let mut cancelled = crate::gui::actions::tests::new_app(&context);
        install_window_target(&mut cancelled);
        cancelled.test_skip_history_persistence = true;
        cancelled.require_confirm_destructive = true;
        let intent =
            install_properties_editor(&mut cancelled, binding.clone(), history_query, false, true);
        let ActionBindingEditorIntent::Test {
            identity,
            binding: test_binding,
            ..
        } = &intent
        else {
            panic!("expected component Test intent")
        };
        assert_eq!(test_binding, &binding);
        let identity = identity.clone();
        route_editor_intent(&mut cancelled, intent);
        assert_eq!(
            cancelled
                .pending_universal_confirm
                .as_ref()
                .map(|pending| { pending.authoring_revalidation.is_some() }),
            Some(true)
        );
        assert!(
            cancelled
                .pending_universal_confirm
                .as_ref()
                .and_then(|pending| pending.authoring_revalidation.as_ref())
                .is_some_and(|revalidation| {
                    revalidation.editor_identity.as_ref() == Some(&identity)
                        && revalidation.binding == binding
                })
        );
        assert!(cancelled.test_activation_trace.is_empty());
        assert!(cancelled.test_recorded_history_queries.is_empty());
        assert!(cancelled.usage.is_empty());
        assert!(cancelled.resolve_pending_universal_action_confirmation(false));
        assert!(cancelled.pending_universal_confirm.is_none());
        assert!(cancelled.test_activation_trace.is_empty());
        assert!(cancelled.test_recorded_history_queries.is_empty());
        assert!(cancelled.usage.is_empty());

        let mut confirmed = crate::gui::actions::tests::new_app(&context);
        install_window_target(&mut confirmed);
        confirmed.test_skip_history_persistence = true;
        confirmed.require_confirm_destructive = true;
        let intent =
            install_properties_editor(&mut confirmed, binding.clone(), history_query, false, true);
        route_editor_intent(&mut confirmed, intent);
        assert!(
            confirmed
                .pending_universal_confirm
                .as_ref()
                .is_some_and(|pending| pending.authoring_revalidation.is_some())
        );
        let received = Arc::new(Mutex::new(Vec::<crate::actions::Action>::new()));
        let callback_received = Arc::clone(&received);
        crate::gui::set_execute_action_hook(Some(Box::new(move |action| {
            callback_received.lock().unwrap().push(action.clone());
            Ok(())
        })));
        let did_confirm = confirmed.resolve_pending_universal_action_confirmation(true);
        crate::gui::set_execute_action_hook(None);

        assert!(did_confirm);
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].action, "window:close:44");
        assert_eq!(
            received[0].args.as_deref(),
            Some("exact unsaved confirmation args")
        );
        assert_eq!(confirmed.test_recorded_history_queries, [history_query]);
        assert_eq!(confirmed.usage.get("window:close:44"), Some(&1));
        assert!(!confirmed.resolve_pending_universal_action_confirmation(true));
        assert_eq!(received.len(), 1);
        assert_eq!(confirmed.test_recorded_history_queries, [history_query]);
        assert_eq!(confirmed.usage.get("window:close:44"), Some(&1));
    }

    #[test]
    fn editing_exact_command_after_test_confirmation_prevents_dispatch() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut app = crate::gui::actions::tests::new_app(&context);
        install_window_target(&mut app);
        app.test_skip_history_persistence = true;
        app.require_confirm_destructive = true;
        let binding = ActionBinding::ExactCommand {
            command: "window:close:44".into(),
            args: Some("confirmation edit args".into()),
        };
        let intent =
            install_properties_editor(&mut app, binding, "m4 confirmation edit", false, true);
        route_editor_intent(&mut app, intent);
        assert!(app.pending_universal_confirm.is_some());

        {
            let mut editor = app.radial_editor.lock().expect("radial editor lock");
            let draft = editor.properties_draft.as_mut().expect("properties draft");
            let scope = draft.action_editor.scope.clone().expect("editor scope");
            let assigned = draft
                .action_editor
                .active_binding
                .clone()
                .expect("assigned exact binding");
            let before = frame(
                &context,
                &mut draft.action_editor,
                scope.clone(),
                &assigned,
                Vec::new(),
            );
            let _ = replace_text(
                &context,
                &mut draft.action_editor,
                &scope,
                &assigned,
                &before,
                "window:close:44",
                "window:show",
            );
            assert_eq!(draft.action_editor.command, "window:show");
        }

        assert!(app.resolve_pending_universal_action_confirmation(true));
        assert!(app.pending_universal_confirm.is_none());
        assert!(app.test_activation_trace.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
        assert!(app.radial_preparations.is_empty());
    }

    #[test]
    fn query_confirmation_provider_drift_falls_back_without_execution() {
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        install_window_target(&mut app);
        app.test_skip_history_persistence = true;
        app.require_confirm_destructive = true;
        let query = "M4 confirmation provider drift";
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: vec![test_action("window:close:44", query)],
            searched: None,
        }));
        let intent = install_properties_editor(&mut app, binding, query, false, true);
        route_editor_intent(&mut app, intent);
        wait_for_authoring_state(&mut app, |app| {
            app.pending_universal_confirm.is_some()
                || app.radial_editor.lock().is_ok_and(|editor| {
                    editor.properties_draft.as_ref().is_some_and(|draft| {
                        draft.action_editor.test_status.as_deref() != Some("Test pending")
                    })
                })
        });
        assert!(app.pending_universal_confirm.is_some());

        app.plugins
            .notify_search_update_for_test("m4_confirmation_drift");
        assert!(app.resolve_pending_universal_action_confirmation(true));

        assert_eq!(app.query, query);
        assert!(app.pending_universal_confirm.is_none());
        assert!(
            !app.test_activation_trace
                .iter()
                .any(|(action, _)| action.action == "window:close:44")
        );
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
        assert!(app.radial_preparations.is_empty());
    }

    #[test]
    fn unchanged_query_confirmation_executes_the_frozen_first_action_once() {
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        install_window_target(&mut app);
        app.test_skip_history_persistence = true;
        app.require_confirm_destructive = true;
        let query = "M4 unchanged query confirmation";
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        let selected = crate::actions::Action {
            label: "M4 frozen first window".into(),
            desc: "M4 query confirmation fixture".into(),
            action: "window:close:44".into(),
            args: Some("frozen query action args".into()),
        };
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: vec![selected.clone()],
            searched: None,
        }));
        let intent = install_properties_editor(&mut app, binding, query, false, true);
        route_editor_intent(&mut app, intent);
        wait_for_authoring_state(&mut app, |app| {
            app.pending_universal_confirm.is_some()
                || app.radial_editor.lock().is_ok_and(|editor| {
                    editor.properties_draft.as_ref().is_some_and(|draft| {
                        draft.action_editor.test_status.as_deref() != Some("Test pending")
                    })
                })
        });
        assert!(
            app.pending_universal_confirm
                .as_ref()
                .and_then(|pending| pending.authoring_revalidation.as_ref())
                .is_some_and(|revalidation| {
                    revalidation.selected_query_action.as_ref() == Some(&selected)
                })
        );

        let received = Arc::new(Mutex::new(Vec::<crate::actions::Action>::new()));
        let callback_received = Arc::clone(&received);
        crate::gui::set_execute_action_hook(Some(Box::new(move |action| {
            callback_received.lock().unwrap().push(action.clone());
            Ok(())
        })));
        assert!(app.resolve_pending_universal_action_confirmation(true));
        crate::gui::set_execute_action_hook(None);

        let received = received.lock().unwrap();
        assert_eq!(received.as_slice(), &[selected.clone()]);
        assert_eq!(
            received[0].args.as_deref(),
            Some("frozen query action args")
        );
        assert_eq!(app.test_recorded_history_queries, [query]);
        assert_eq!(app.usage.get("window:close:44"), Some(&1));
        assert!(!app.resolve_pending_universal_action_confirmation(true));
        assert_eq!(received.len(), 1);
        assert_eq!(app.test_recorded_history_queries, [query]);
        assert_eq!(app.usage.get("window:close:44"), Some(&1));
    }

    #[test]
    fn changed_window_identity_at_query_confirmation_blocks_dispatch() {
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        install_window_target(&mut app);
        app.test_skip_history_persistence = true;
        app.require_confirm_destructive = true;
        let query = "M4 window identity confirmation drift";
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: vec![test_action("window:close:44", query)],
            searched: None,
        }));
        let intent = install_properties_editor(&mut app, binding, query, false, true);
        route_editor_intent(&mut app, intent);
        wait_for_authoring_state(&mut app, |app| {
            app.pending_universal_confirm.is_some()
                || app.radial_editor.lock().is_ok_and(|editor| {
                    editor.properties_draft.as_ref().is_some_and(|draft| {
                        draft.action_editor.test_status.as_deref() != Some("Test pending")
                    })
                })
        });
        assert!(app.pending_universal_confirm.is_some());
        let provider_revision = app.plugins.search_generation();
        let catalog_versions = crate::radial::dynamic::MutableResultCatalogVersions::current();

        let replacement = crate::window_catalog::WindowDescriptor {
            title: "Replacement window".into(),
            hwnd: 44,
            pid: 7,
            executable: Some("editor.exe".into()),
            process_path: Some("C:\\Apps\\editor.exe".into()),
            class_name: Some("ReplacementWindow".into()),
        };
        let current = replacement.clone();
        app.plugins.set_window_catalog_for_test(
            crate::window_catalog::WindowCatalog::from_snapshot_with_descriptor(
                vec![replacement],
                move |hwnd| (hwnd == current.hwnd).then(|| current.clone()),
            ),
        );
        assert_eq!(app.plugins.search_generation(), provider_revision);
        assert_eq!(
            crate::radial::dynamic::MutableResultCatalogVersions::current(),
            catalog_versions
        );
        assert!(app.resolve_pending_universal_action_confirmation(true));

        assert_eq!(app.query, query);
        assert!(app.pending_universal_confirm.is_none());
        assert!(
            !app.test_activation_trace
                .iter()
                .any(|(action, _)| action.action == "window:close:44")
        );
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
        assert!(app.radial_preparations.is_empty());
    }

    #[test]
    fn changed_mutable_catalog_at_query_confirmation_blocks_dispatch() {
        let context = egui::Context::default();
        let mut app = crate::gui::actions::tests::new_app(&context);
        install_window_target(&mut app);
        app.test_skip_history_persistence = true;
        app.require_confirm_destructive = true;
        let query = "M4 mutable catalog confirmation drift";
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: vec![test_action("window:close:44", query)],
            searched: None,
        }));
        let intent = install_properties_editor(&mut app, binding, query, false, true);
        route_editor_intent(&mut app, intent);
        wait_for_authoring_state(&mut app, |app| {
            app.pending_universal_confirm.is_some()
                || app.radial_editor.lock().is_ok_and(|editor| {
                    editor.properties_draft.as_ref().is_some_and(|draft| {
                        draft.action_editor.test_status.as_deref() != Some("Test pending")
                    })
                })
        });
        let pending = app
            .pending_universal_confirm
            .as_ref()
            .expect("query Test should wait for confirmation");
        let captured_versions = pending
            .authoring_revalidation
            .as_ref()
            .and_then(|revalidation| revalidation.result_catalog_versions)
            .expect("query confirmation retains the captured catalog versions");
        let provider_revision = app.plugins.search_generation();
        assert_eq!(
            captured_versions,
            crate::radial::dynamic::MutableResultCatalogVersions::current()
        );

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("m4-authoring-clipboard-history.json");
        crate::plugins::clipboard::save_history(
            path.to_str().unwrap(),
            &std::collections::VecDeque::from(["M4 new clipboard entry".to_owned()]),
        )
        .unwrap();
        assert_ne!(
            captured_versions,
            crate::radial::dynamic::MutableResultCatalogVersions::current()
        );
        assert_eq!(app.plugins.search_generation(), provider_revision);
        assert!(app.resolve_pending_universal_action_confirmation(true));

        assert_eq!(app.query, query);
        assert!(app.pending_universal_confirm.is_none());
        assert!(
            !app.test_activation_trace
                .iter()
                .any(|(action, _)| action.action == "window:close:44")
        );
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
        assert!(app.radial_preparations.is_empty());
    }

    #[test]
    fn close_keep_editing_retires_blocked_test_and_wakes_latest_preview_demand() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut app = crate::gui::actions::tests::new_app(&context);
        app.test_skip_history_persistence = true;
        let (first_started_tx, first_started_rx) = mpsc::channel();
        let (release_first_tx, release_first_rx) = mpsc::channel();
        let (latest_started_tx, latest_started_rx) = mpsc::channel();
        app.plugins.register(Box::new(CloseThenSearchPlugin {
            first_started: first_started_tx,
            release_first: Mutex::new(release_first_rx),
            latest_started: latest_started_tx,
            result: test_action("help:show", "M4 queued latest result"),
            calls: std::sync::atomic::AtomicUsize::new(0),
        }));
        let query = "M4 retained draft query";
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        let intent = install_properties_editor(&mut app, binding.clone(), query, true, true);
        let ActionBindingEditorIntent::Test { identity, .. } = &intent else {
            panic!("expected component Test intent")
        };
        let identity = identity.clone();
        route_editor_intent(&mut app, intent);
        first_started_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("the original provider request should begin");
        assert!(app.radial_provider_search_capacity.is_occupied());

        let original_session = {
            let mut editor = app.radial_editor.lock().expect("radial editor lock");
            let session = editor.session.as_ref().unwrap().editor_session;
            editor.request_close();
            assert!(editor.close_prompt);
            let prompt = prompt_frame(&context, &mut editor, Vec::new());
            click_button(&context, &mut editor, &prompt, "Keep editing");
            assert!(!editor.close_prompt);
            assert_eq!(editor.session.as_ref().unwrap().editor_session, session);
            assert_eq!(
                editor
                    .properties_draft
                    .as_ref()
                    .unwrap()
                    .action_editor
                    .query,
                query
            );
            assert!(!editor.action_test_request_is_current(&identity, &binding));
            session
        };

        let (latest_identity, latest_query) = {
            let mut editor = app.radial_editor.lock().expect("radial editor lock");
            assert_eq!(
                editor.session.as_ref().unwrap().editor_session,
                original_session
            );
            let action_editor = &mut editor.properties_draft.as_mut().unwrap().action_editor;
            let mut intents = Vec::new();
            action_editor.emit_search(&mut intents);
            let ActionBindingEditorIntent::Search { identity, query } =
                intents.into_iter().next().unwrap()
            else {
                panic!("expected the latest Search intent")
            };
            (identity, query)
        };
        route_editor_intent(
            &mut app,
            ActionBindingEditorIntent::Search {
                identity: latest_identity,
                query: latest_query,
            },
        );
        assert!(app.radial_provider_search_capacity.is_occupied());
        assert!(latest_started_rx.try_recv().is_err());

        release_first_tx
            .send(())
            .expect("release canceled provider");
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        let mut latest_started = false;
        while Instant::now() < deadline && !latest_started {
            app.process_watch_events();
            latest_started = latest_started_rx.try_recv().is_ok();
            if !latest_started {
                std::thread::yield_now();
            }
        }
        assert!(
            latest_started,
            "capacity release should start latest demand"
        );
        wait_for_authoring_state(&mut app, |app| {
            app.radial_editor.lock().is_ok_and(|editor| {
                editor.properties_draft.as_ref().is_some_and(|draft| {
                    matches!(&draft.action_editor.search_status, SearchStatus::Results)
                })
            })
        });

        assert_eq!(app.test_activation_trace.len(), 0);
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
        assert!(app.radial_preparations.is_empty());
        assert_eq!(app.query, "");
    }

    #[test]
    fn discard_reopen_rejects_already_queued_search_and_test_completions() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut app = crate::gui::actions::tests::new_app(&context);
        app.test_skip_history_persistence = true;
        let query = "m4-discard-reopen-retired-query";
        let binding = ActionBinding::LauncherQuery {
            query: query.into(),
            mode: QueryRunMode::ExecuteFirst,
        };
        app.plugins.register(Box::new(AuthoringSearchPlugin {
            results: vec![test_action("exec:m4_retired_completion", query)],
            searched: None,
        }));
        let search_intent =
            install_properties_editor(&mut app, binding.clone(), query, true, false);
        route_editor_intent(&mut app, search_intent);
        let old_search_event = receive_authoring_search_result(&mut app);
        assert!(matches!(
            &old_search_event,
            crate::gui::WatchEvent::RadialAuthoringSearchReady { .. }
        ));

        let original_session = {
            let mut editor = app.radial_editor.lock().expect("radial editor lock");
            let session = editor.session.as_ref().unwrap().editor_session;
            editor.request_close();
            let prompt = prompt_frame(&context, &mut editor, Vec::new());
            click_button(&context, &mut editor, &prompt, "Keep editing");
            assert_eq!(editor.session.as_ref().unwrap().editor_session, session);
            assert!(!editor.close_prompt);
            session
        };
        deliver_watch_event(&mut app, old_search_event);
        {
            let editor = app.radial_editor.lock().expect("radial editor lock");
            let draft = editor.properties_draft.as_ref().unwrap();
            assert_eq!(draft.action_editor.query, query);
            assert!(matches!(
                draft.action_editor.search_status,
                SearchStatus::Idle
            ));
            assert!(draft.action_editor.search_rows.is_empty());
        }

        let test_intent = {
            let mut editor = app.radial_editor.lock().expect("radial editor lock");
            let action_editor = &mut editor.properties_draft.as_mut().unwrap().action_editor;
            let mut intents = Vec::new();
            action_editor.emit_test(binding, &mut intents);
            intents
                .into_iter()
                .next()
                .expect("fresh Test intent after Keep")
        };
        route_editor_intent(&mut app, test_intent);
        let old_test_event = receive_authoring_search_result(&mut app);
        assert!(matches!(
            &old_test_event,
            crate::gui::WatchEvent::RadialAuthoringSearchReady { .. }
        ));

        let new_session = {
            let mut editor = app.radial_editor.lock().expect("radial editor lock");
            editor.request_close();
            let prompt = prompt_frame(&context, &mut editor, Vec::new());
            click_button(&context, &mut editor, &prompt, "Discard");
            assert!(!editor.open);
            assert!(editor.session.is_none());
            editor.open();
            editor.session.as_ref().unwrap().editor_session
        };
        assert_ne!(new_session, original_session);
        deliver_watch_event(&mut app, old_test_event);

        let editor = app.radial_editor.lock().expect("radial editor lock");
        assert!(editor.open);
        assert_eq!(editor.session.as_ref().unwrap().editor_session, new_session);
        assert!(editor.properties_draft.is_none());
        assert_eq!(app.query, "");
        assert!(app.test_activation_trace.is_empty());
        assert!(app.test_recorded_history_queries.is_empty());
        assert!(app.usage.is_empty());
        assert!(app.radial_preparations.is_empty());
    }
}
