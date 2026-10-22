use crate::gui::LauncherApp;
use crate::gui::confirmation_modal::{ConfirmationModal, ConfirmationResult, DestructiveAction};
use crate::gui::jump_mode::ui::{
    FocusDirective, JumpModeScope, NavigationOwner, register_and_apply_target,
};
use crate::gui::jump_mode::{SurfaceId, TargetGeneration, TargetKind};
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, labeled_form_row, secondary_help,
    semantic_status, with_button_emphasis,
};
use crate::mouse_gestures::db::{
    BindingEntry, BindingKind, GESTURES_FILE, GestureDb, GestureEntry, format_gesture_label,
    load_gestures,
};
use crate::mouse_gestures::engine::{DirMode, GestureTracker};
use crate::mouse_gestures::service::MouseGestureConfig;
use crate::plugin::Plugin;
use eframe::egui;

#[derive(Debug, Clone, Copy)]
pub struct RecorderConfig {
    threshold_px: f32,
    long_threshold_x: f32,
    long_threshold_y: f32,
    max_tokens: usize,
}

impl RecorderConfig {
    pub fn new(
        threshold_px: f32,
        long_threshold_x: f32,
        long_threshold_y: f32,
        max_tokens: usize,
    ) -> Self {
        Self {
            threshold_px,
            long_threshold_x,
            long_threshold_y,
            max_tokens,
        }
    }

    pub fn from_gesture_config(config: &MouseGestureConfig) -> Self {
        Self {
            threshold_px: config.threshold_px,
            long_threshold_x: config.long_threshold_x,
            long_threshold_y: config.long_threshold_y,
            max_tokens: config.max_tokens,
        }
    }

    pub fn tracker(&self, dir_mode: DirMode) -> GestureTracker {
        GestureTracker::new(
            dir_mode,
            self.threshold_px,
            self.long_threshold_x,
            self.long_threshold_y,
            self.max_tokens,
        )
    }
}

pub fn default_recorder_config() -> RecorderConfig {
    RecorderConfig::from_gesture_config(&MouseGestureConfig::default())
}

const STROKE_MAX_POINTS: usize = 256;
const STROKE_I16_SCALE: f32 = 32767.0;

fn normalize_stroke_points(points: &[egui::Pos2], max_points: usize) -> Vec<[i16; 2]> {
    if points.len() < 2 {
        return Vec::new();
    }

    // Downsample (cheap) so we don't bloat the JSON file when drawing long gestures.
    let step = (points.len() / max_points).max(1);
    let mut sampled: Vec<egui::Pos2> = points.iter().copied().step_by(step).collect();
    if sampled.last().copied() != points.last().copied()
        && let Some(last) = points.last().copied()
    {
        sampled.push(last);
    }

    let mut min = sampled[0];
    let mut max = sampled[0];
    for p in &sampled[1..] {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
    }
    let center = egui::pos2((min.x + max.x) * 0.5, (min.y + max.y) * 0.5);
    let half_span = ((max.x - min.x).max(max.y - min.y)) * 0.5;
    if half_span <= f32::EPSILON {
        return Vec::new();
    }

    let mut out: Vec<[i16; 2]> = Vec::with_capacity(sampled.len());
    for p in sampled {
        let nx = ((p.x - center.x) / half_span).clamp(-1.0, 1.0);
        let ny = ((p.y - center.y) / half_span).clamp(-1.0, 1.0);
        let ix = (nx * STROKE_I16_SCALE).round() as i16;
        let iy = (ny * STROKE_I16_SCALE).round() as i16;
        if out.last().copied() != Some([ix, iy]) {
            out.push([ix, iy]);
        }
    }

    out
}

fn stroke_points_in_rect(stroke: &[[i16; 2]], rect: egui::Rect) -> Vec<egui::Pos2> {
    if stroke.len() < 2 {
        return Vec::new();
    }

    let rect = rect.shrink(12.0);
    let center = rect.center();
    let scale = rect.width().min(rect.height()) * 0.45;
    if scale <= f32::EPSILON {
        return Vec::new();
    }

    stroke
        .iter()
        .map(|p| {
            let nx = p[0] as f32 / STROKE_I16_SCALE;
            let ny = p[1] as f32 / STROKE_I16_SCALE;
            egui::pos2(center.x + nx * scale, center.y + ny * scale)
        })
        .collect()
}

const AUTO_LABEL_PREFIX: &str = "Gesture";
const AUTO_LABEL_TOKEN_MAX: usize = 24;

fn register_mouse_gesture_target(
    scope: Option<&JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: impl AsRef<str>,
    label: &'static str,
    kind: TargetKind,
    visual_order: &mut u64,
) -> Option<FocusDirective> {
    let identity = identity.as_ref();
    let directive =
        register_and_apply_target(scope, ui, response, identity, label, kind, visual_order);
    if directive == Some(FocusDirective::Select)
        && let Some(scope) = scope
    {
        let target_id = scope.target_id(identity);
        let _ = scope.record_select_focus_owner(&target_id, response.id);
    }
    directive
}

fn normalize_recorded_tokens_for_label(tokens: &str) -> String {
    tokens
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .take(AUTO_LABEL_TOKEN_MAX)
        .collect()
}

fn label_from_recorded_tokens(tokens: &str) -> Option<String> {
    let normalized = normalize_recorded_tokens_for_label(tokens);
    if normalized.is_empty() {
        None
    } else {
        Some(format!("{AUTO_LABEL_PREFIX} {normalized}"))
    }
}

fn is_default_generated_label(label: &str) -> bool {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return true;
    }

    let Some(rest) = trimmed.strip_prefix(AUTO_LABEL_PREFIX) else {
        return false;
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return true;
    }

    rest.chars().all(|c| c.is_ascii_digit()) || rest == normalize_recorded_tokens_for_label(rest)
}

fn list_query_prefix_for_plugin(plugin_name: &str) -> Option<(&'static str, Option<&'static str>)> {
    match plugin_name {
        "folders" => Some(("f list", Some("f"))),
        "bookmarks" => Some(("bm list", Some("bm"))),
        "clipboard" => Some(("cb list", Some("cb"))),
        "emoji" => Some(("emoji list", Some("emoji"))),
        "note" | "notes" => Some(("note list", Some("note search"))),
        "processes" => Some(("ps", Some("pss"))),
        "todo" => Some(("todo list", Some("todo"))),
        "snippets" => Some(("cs list", Some("cs"))),
        _ => None,
    }
}

fn has_concrete_actions(actions: &[crate::actions::Action]) -> bool {
    actions
        .iter()
        .any(|action| !action.action.trim_start().starts_with("query:"))
}

fn resolve_action_source(plugin: &dyn Plugin, filter: &str) -> Vec<crate::actions::Action> {
    if let Some((prefix, alternate_prefix)) = list_query_prefix_for_plugin(plugin.name()) {
        let filter = filter.trim();
        for candidate in [Some(prefix), alternate_prefix] {
            let Some(candidate) = candidate else {
                continue;
            };

            let query = if filter.is_empty() {
                candidate.to_string()
            } else {
                format!("{candidate} {filter}")
            };
            let actions = plugin.search(&query);
            if actions.is_empty() {
                continue;
            }
            if has_concrete_actions(&actions) {
                return actions;
            }
        }
    }
    plugin.commands()
}

fn append_query_args(query: &str, add_args: &str) -> String {
    let extra = add_args.trim();
    if extra.is_empty() {
        return query.to_string();
    }
    if query.ends_with(' ') {
        format!("{query}{extra}")
    } else {
        format!("{query} {extra}")
    }
}

fn apply_recording_to_entry(
    entry: &mut GestureEntry,
    token_buffer: &mut String,
    recorded_tokens: &str,
    normalized_stroke: Vec<[i16; 2]>,
) {
    entry.tokens = recorded_tokens.to_string();
    *token_buffer = entry.tokens.clone();
    entry.stroke = normalized_stroke;

    if is_default_generated_label(&entry.label)
        && let Some(auto_label) = label_from_recorded_tokens(recorded_tokens)
    {
        entry.label = auto_label;
    }
}

fn apply_action_pick(editor: &mut BindingEditor, act: &crate::actions::Action, add_args: &str) {
    if let Some(query) = act.action.strip_prefix("query:") {
        editor.kind = match editor.kind {
            BindingKind::SetQueryAndShow => BindingKind::SetQueryAndShow,
            BindingKind::SetQueryAndExecute => BindingKind::SetQueryAndExecute,
            _ => BindingKind::SetQuery,
        };
        editor.action = append_query_args(query, add_args);
        editor.args.clear();
    } else if act.action == "launcher:toggle" {
        editor.kind = BindingKind::ToggleLauncher;
        editor.action.clear();
        editor.args.clear();
    } else {
        editor.kind = BindingKind::Execute;
        editor.action = act.action.clone();
        editor.args = if add_args.trim().is_empty() {
            act.args.clone().unwrap_or_default()
        } else {
            add_args.trim().to_string()
        };
    }
    editor.label = act.label.clone();
    editor.add_args.clear();
}

fn opaque_choice_identity<T: std::hash::Hash>(value: &T) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn action_choice_identity(action: &crate::actions::Action, occurrence: usize) -> u64 {
    opaque_choice_identity(&(
        action.action.as_str(),
        action.args.as_deref(),
        action.label.as_str(),
        action.desc.as_str(),
        occurrence,
    ))
}

fn activation_key_is_held(ctx: &egui::Context) -> bool {
    ctx.input(|input| input.key_down(egui::Key::Enter) || input.key_down(egui::Key::Space))
}

pub struct GestureRecorder {
    config: RecorderConfig,
    tracker: GestureTracker,
    points: Vec<egui::Pos2>,
    draw_points: Vec<egui::Pos2>,
    next_time_ms: u64,
}

impl GestureRecorder {
    pub fn new(dir_mode: DirMode, config: RecorderConfig) -> Self {
        let tracker = config.tracker(dir_mode);
        Self {
            config,
            tracker,
            points: Vec::new(),
            draw_points: Vec::new(),
            next_time_ms: 0,
        }
    }

    pub fn with_tracker(tracker: GestureTracker, config: RecorderConfig) -> Self {
        Self {
            config,
            tracker,
            points: Vec::new(),
            draw_points: Vec::new(),
            next_time_ms: 0,
        }
    }

    pub fn tokens_string(&self) -> String {
        self.tracker.tokens_string()
    }

    pub fn points(&self) -> &[egui::Pos2] {
        &self.draw_points
    }

    pub fn normalized_stroke(&self) -> Vec<[i16; 2]> {
        normalize_stroke_points(&self.draw_points, STROKE_MAX_POINTS)
    }

    pub fn reset(&mut self) {
        self.tracker.reset();
        self.points.clear();
        self.draw_points.clear();
        self.next_time_ms = 0;
    }

    pub fn set_dir_mode(&mut self, dir_mode: DirMode) {
        self.tracker = self.config.tracker(dir_mode);
        self.reset();
    }

    pub fn push_point(&mut self, pos: egui::Pos2) -> Option<char> {
        self.points.push(pos);
        self.extend_draw_points(pos);
        let time = self.next_time_ms;
        self.next_time_ms = self.next_time_ms.saturating_add(16);
        self.tracker.feed_point((pos.x, pos.y), time)
    }

    fn extend_draw_points(&mut self, pos: egui::Pos2) {
        let step = 4.0;
        if let Some(last) = self.draw_points.last().copied() {
            let delta = pos - last;
            let distance = delta.length();
            if distance > step {
                let steps = (distance / step).ceil() as usize;
                for i in 1..steps {
                    let t = i as f32 / steps as f32;
                    self.draw_points.push(last + delta * t);
                }
            }
        }
        self.draw_points.push(pos);
    }
}

#[derive(Clone, Default)]
struct BindingEditor {
    edit_idx: Option<usize>,
    label: String,
    action: String,
    args: String,
    enabled: bool,
    kind: BindingKind,
    add_plugin: String,
    add_filter: String,
    add_args: String,
    focus_label: bool,
}

impl BindingEditor {
    fn reset(&mut self) {
        self.edit_idx = None;
        self.label.clear();
        self.action.clear();
        self.args.clear();
        self.enabled = true;
        self.kind = BindingKind::Execute;
        self.add_plugin.clear();
        self.add_filter.clear();
        self.add_args.clear();
        self.focus_label = false;
    }

    fn start_edit(&mut self, binding: Option<&BindingEntry>, idx: usize) {
        if let Some(binding) = binding {
            self.label = binding.label.clone();
            self.kind = binding.kind;
            self.action = binding.action.clone();
            self.args = if binding.kind == BindingKind::Execute {
                binding.args.clone().unwrap_or_default()
            } else {
                String::new()
            };
            self.enabled = binding.enabled;
        } else {
            self.label.clear();
            self.action.clear();
            self.args.clear();
            self.enabled = true;
            self.kind = BindingKind::Execute;
        }
        self.edit_idx = Some(idx);
        self.add_plugin.clear();
        self.add_filter.clear();
        self.add_args.clear();
        self.focus_label = true;
    }
}

#[derive(Default)]
struct BindingDialog {
    open: bool,
    activation_key_release_required: bool,
    gesture_idx: Option<usize>,
    editor: BindingEditor,
    instance: u64,
    picker_revision: u64,
    combo_popup_ids: Vec<egui::Id>,
}

impl BindingDialog {
    fn renew_instance(&mut self) -> u64 {
        self.instance = self.instance.wrapping_add(1).max(1);
        self.combo_popup_ids.clear();
        self.instance
    }

    fn open_new(
        &mut self,
        gesture_idx: usize,
        next_idx: usize,
        activation_key_release_required: bool,
    ) {
        self.renew_instance();
        self.open = true;
        self.activation_key_release_required = activation_key_release_required;
        self.gesture_idx = Some(gesture_idx);
        self.picker_revision = self.picker_revision.wrapping_add(1).max(1);
        self.editor.start_edit(None, next_idx);
    }

    fn open_edit(
        &mut self,
        gesture_idx: usize,
        binding: &BindingEntry,
        edit_idx: usize,
        activation_key_release_required: bool,
    ) {
        self.renew_instance();
        self.open = true;
        self.activation_key_release_required = activation_key_release_required;
        self.gesture_idx = Some(gesture_idx);
        self.picker_revision = self.picker_revision.wrapping_add(1).max(1);
        self.editor.start_edit(Some(binding), edit_idx);
    }
}

struct MouseGesturesDraftSnapshot {
    rename_label: String,
    token_buffer: String,
    binding_editor: BindingEditor,
    picker_revision: u64,
}

impl MouseGesturesDraftSnapshot {
    fn capture(dialog: &MgGesturesDialog) -> Self {
        Self {
            rename_label: dialog.rename_label.clone(),
            token_buffer: dialog.token_buffer.clone(),
            binding_editor: dialog.binding_dialog.editor.clone(),
            picker_revision: dialog.binding_dialog.picker_revision,
        }
    }

    fn restore(self, dialog: &mut MgGesturesDialog) {
        dialog.rename_label = self.rename_label;
        dialog.token_buffer = self.token_buffer;
        dialog.binding_dialog.editor = self.binding_editor;
        dialog.binding_dialog.picker_revision = self.picker_revision;
    }
}

pub struct MgGesturesDialog {
    pub open: bool,
    db: GestureDb,
    committed_db: GestureDb,
    load_error: Option<String>,
    selected_idx: Option<usize>,
    rename_idx: Option<usize>,
    rename_label: String,
    recorder: GestureRecorder,
    token_buffer: String,
    binding_dialog: BindingDialog,
    delete_confirm_modal: ConfirmationModal,
    pending_delete: Option<PendingGestureDelete>,
    target_generation: u64,
    next_identity: u64,
    gesture_tokens: Vec<u64>,
    binding_tokens: Vec<Vec<u64>>,
    delete_confirmation_instance: u64,
    main_combo_popup_ids: Vec<egui::Id>,
    canvas_drag_active: bool,
    #[cfg(test)]
    last_right_panel_scroll_area_id: Option<egui::Id>,
    #[cfg(test)]
    last_right_panel_scroll_max_offset_y: f32,
}

#[derive(Debug)]
struct PendingGestureDelete {
    idx: usize,
    token: u64,
    expected: GestureEntry,
    confirmation_instance: u64,
    activation_key_release_required: bool,
}

impl Default for MgGesturesDialog {
    fn default() -> Self {
        let config = default_recorder_config();
        Self {
            open: false,
            db: GestureDb::default(),
            committed_db: GestureDb::default(),
            load_error: None,
            selected_idx: None,
            rename_idx: None,
            rename_label: String::new(),
            recorder: GestureRecorder::new(DirMode::Four, config),
            token_buffer: String::new(),
            binding_dialog: BindingDialog::default(),
            delete_confirm_modal: ConfirmationModal::default(),
            pending_delete: None,
            target_generation: 1,
            next_identity: 1,
            gesture_tokens: Vec::new(),
            binding_tokens: Vec::new(),
            delete_confirmation_instance: 0,
            main_combo_popup_ids: Vec::new(),
            canvas_drag_active: false,
            #[cfg(test)]
            last_right_panel_scroll_area_id: None,
            #[cfg(test)]
            last_right_panel_scroll_max_offset_y: 0.0,
        }
    }
}

impl MgGesturesDialog {
    #[cfg(test)]
    pub(crate) fn test_right_panel_scroll_area_id(&self) -> Option<egui::Id> {
        self.last_right_panel_scroll_area_id
    }

    #[cfg(test)]
    pub(crate) fn test_right_panel_scroll_max_offset_y(&self) -> f32 {
        self.last_right_panel_scroll_max_offset_y
    }

    fn allocate_identity(&mut self) -> u64 {
        let identity = self.next_identity.max(1);
        self.next_identity = identity.wrapping_add(1).max(1);
        identity
    }

    fn advance_target_generation(&mut self) {
        self.target_generation = self.target_generation.wrapping_add(1).max(1);
    }

    pub(crate) fn jump_mode_target_generation(&self) -> TargetGeneration {
        TargetGeneration(self.target_generation.max(1))
    }

    pub(crate) fn recorder_canvas_active(&self) -> bool {
        self.canvas_drag_active
    }

    pub(crate) fn reconcile_canvas_drag_with_root_lifecycle(
        &mut self,
        ctx: &egui::Context,
        root_visible: bool,
    ) {
        if self.canvas_drag_active
            && (!root_visible || !self.open || !ctx.input(|input| input.pointer.any_down()))
        {
            self.canvas_drag_active = false;
        }
    }

    #[cfg(test)]
    pub(crate) fn test_set_canvas_drag_active(&mut self, active: bool) {
        self.canvas_drag_active = active;
    }

    #[cfg(test)]
    pub(crate) fn test_selected_gesture_label(&self) -> Option<&str> {
        self.selected_idx
            .and_then(|idx| self.db.gestures.get(idx))
            .map(|gesture| gesture.label.as_str())
    }

    pub(crate) fn jump_mode_child_owner(&self, parent_instance: u64) -> Option<NavigationOwner> {
        if let Some(pending) = &self.pending_delete {
            Some(NavigationOwner::MouseGestureDelete {
                parent_instance,
                confirmation_instance: pending.confirmation_instance,
            })
        } else if self.binding_dialog.open {
            Some(NavigationOwner::MouseGestureBinding {
                parent_instance,
                binding_instance: self.binding_dialog.instance,
            })
        } else {
            None
        }
    }

    pub(crate) fn jump_mode_binding_popup_is_open(&self, ctx: &egui::Context) -> bool {
        self.binding_dialog
            .combo_popup_ids
            .iter()
            .copied()
            .any(|id| egui::Popup::is_id_open(ctx, id))
    }

    #[cfg(test)]
    pub(crate) fn test_binding_category(&self) -> Option<&str> {
        self.binding_dialog
            .open
            .then_some(self.binding_dialog.editor.add_plugin.as_str())
    }

    pub(crate) fn jump_mode_main_popup_is_open(&self, ctx: &egui::Context) -> bool {
        self.main_combo_popup_ids
            .iter()
            .copied()
            .any(|id| egui::Popup::is_id_open(ctx, id))
    }

    fn ensure_identity_tokens(&mut self) {
        if self.gesture_tokens.len() != self.db.gestures.len() {
            self.gesture_tokens.clear();
            self.binding_tokens.clear();
            for gesture_idx in 0..self.db.gestures.len() {
                let token = self.allocate_identity();
                self.gesture_tokens.push(token);
                let binding_count = self.db.gestures[gesture_idx].bindings.len();
                let mut tokens = Vec::with_capacity(binding_count);
                for _ in 0..binding_count {
                    tokens.push(self.allocate_identity());
                }
                self.binding_tokens.push(tokens);
            }
            self.advance_target_generation();
            return;
        }
        if self.binding_tokens.len() != self.db.gestures.len() {
            self.binding_tokens
                .resize_with(self.db.gestures.len(), Vec::new);
        }
        for gesture_idx in 0..self.db.gestures.len() {
            let binding_count = self.db.gestures[gesture_idx].bindings.len();
            if self.binding_tokens[gesture_idx].len() > binding_count {
                self.binding_tokens[gesture_idx].clear();
                for _ in 0..binding_count {
                    let token = self.allocate_identity();
                    self.binding_tokens[gesture_idx].push(token);
                }
                self.advance_target_generation();
            } else if self.binding_tokens[gesture_idx].len() < binding_count {
                while self.binding_tokens[gesture_idx].len() < binding_count {
                    let token = self.allocate_identity();
                    self.binding_tokens[gesture_idx].push(token);
                }
                self.advance_target_generation();
            }
        }
    }

    fn reset_identity_tokens(&mut self) {
        self.gesture_tokens.clear();
        self.binding_tokens.clear();
        self.ensure_identity_tokens();
        self.advance_target_generation();
    }

    fn gesture_token(&self, idx: usize) -> Option<u64> {
        self.gesture_tokens.get(idx).copied()
    }

    fn binding_token(&self, gesture_idx: usize, binding_idx: usize) -> Option<u64> {
        self.binding_tokens
            .get(gesture_idx)
            .and_then(|tokens| tokens.get(binding_idx))
            .copied()
    }

    /// Returns gesture indices sorted by label (case-insensitive) for display purposes.
    fn sorted_gesture_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.db.gestures.len()).collect();
        indices.sort_by_key(|&i| self.db.gestures[i].label.to_lowercase());
        indices
    }

    pub fn open(&mut self) {
        self.canvas_drag_active = false;
        self.reload_from(GESTURES_FILE);
        self.finish_opening();
    }

    fn open_with_loaded_database(&mut self, database: GestureDb) {
        self.canvas_drag_active = false;
        self.install_loaded_database(database);
        self.finish_opening();
    }

    fn finish_opening(&mut self) {
        self.open = true;
        self.selected_idx = self.sorted_gesture_indices().into_iter().next();
        self.rename_idx = None;
        self.rename_label.clear();
        self.token_buffer.clear();
        self.binding_dialog.open = false;
        self.binding_dialog.editor.reset();
        self.pending_delete = None;
        self.delete_confirm_modal.close();
        self.ensure_selection();
    }

    fn reset_database_editor_state(&mut self) {
        self.pending_delete = None;
        self.delete_confirm_modal.close();
        self.binding_dialog.open = false;
        self.binding_dialog.editor.reset();
    }

    fn install_loaded_database(&mut self, db: GestureDb) {
        self.reset_database_editor_state();
        self.committed_db = db.clone();
        self.db = db;
        self.load_error = None;
        self.reset_identity_tokens();
    }

    fn reload_from(&mut self, path: impl AsRef<std::path::Path>) {
        match load_gestures(path) {
            Ok(db) => {
                self.install_loaded_database(db);
            }
            Err(error) => {
                self.reset_database_editor_state();
                self.db = self.committed_db.clone();
                self.load_error = Some(format!("Failed to load mouse gestures: {error}"));
                self.reset_identity_tokens();
            }
        }
    }

    pub fn open_focus(&mut self, label: &str, tokens: &str, dir_mode: DirMode) {
        self.open();
        self.selected_idx = self
            .db
            .gestures
            .iter()
            .position(|gesture| {
                gesture.label == label && gesture.tokens == tokens && gesture.dir_mode == dir_mode
            })
            .or(self.selected_idx);
        self.ensure_selection();
    }

    pub fn open_add(&mut self) {
        self.open();
        if self.load_error.is_none() {
            self.add_gesture();
        }
    }

    pub fn open_binding_editor(&mut self) {
        self.open();
        if self.load_error.is_some() {
            return;
        }
        if self.db.gestures.is_empty() {
            self.add_gesture();
        } else {
            self.ensure_selection();
        }
        if let Some(idx) = self.selected_idx {
            let next_idx = self.db.gestures[idx].bindings.len();
            self.binding_dialog.open_new(idx, next_idx, false);
        }
    }

    /// Reveals an existing binding from the Hotkey Directory while retaining
    /// an open gesture draft. This never creates a gesture or binding.
    pub(crate) fn reveal_binding_for_hotkey_directory(
        &mut self,
        publication: u64,
        entry_id: &crate::hotkey::directory::EntryId,
        gesture_index: usize,
        binding_index: usize,
    ) -> Result<(), &'static str> {
        self.reveal_binding_with_hotkey_directory_source(
            publication,
            entry_id,
            gesture_index,
            binding_index,
            crate::plugins::mouse_gestures::hotkey_directory_source,
            crate::plugins::mouse_gestures::hotkey_directory_binding_matches_committed,
            || {
                load_gestures(GESTURES_FILE)
                    .map_err(|_| "Mouse Gestures could not load the current database.")
            },
        )
    }

    fn reveal_binding_with_hotkey_directory_source(
        &mut self,
        publication: u64,
        entry_id: &crate::hotkey::directory::EntryId,
        gesture_index: usize,
        binding_index: usize,
        mut current_source: impl FnMut() -> crate::hotkey::directory::SourceSnapshot,
        mut current_owner_database_matches: impl FnMut(
            u64,
            &crate::hotkey::directory::EntryId,
            usize,
            usize,
            &GestureDb,
        ) -> bool,
        mut load_closed_database: impl FnMut() -> Result<GestureDb, &'static str>,
    ) -> Result<(), &'static str> {
        if !hotkey_directory_binding_is_current(
            current_source(),
            publication,
            entry_id,
            gesture_index,
            binding_index,
        ) {
            return Err("The requested gesture binding changed after this snapshot.");
        }

        // A closed dialog will reload its file on open. Validate that exact
        // prospective baseline against the runtime owner before changing any
        // dialog state, since the file watcher may not have published it yet.
        let closed_database = if self.open {
            None
        } else {
            Some(load_closed_database()?)
        };
        let database_baseline = closed_database.as_ref().unwrap_or(&self.committed_db);
        if !current_owner_database_matches(
            publication,
            entry_id,
            gesture_index,
            binding_index,
            database_baseline,
        ) {
            return Err("The requested gesture binding changed in the current owner database.");
        }
        if let Some(database) = closed_database {
            self.open_with_loaded_database(database);
        }

        // Confirm the same owner publication and committed target again after
        // any validated closed-dialog load, before opening the child editor.
        if !hotkey_directory_binding_is_current(
            current_source(),
            publication,
            entry_id,
            gesture_index,
            binding_index,
        ) {
            return Err("The requested gesture binding changed after this snapshot.");
        }
        if !current_owner_database_matches(
            publication,
            entry_id,
            gesture_index,
            binding_index,
            &self.committed_db,
        ) {
            return Err("The requested gesture binding changed in the current owner database.");
        }
        if self.load_error.is_some() {
            return Err("Mouse Gestures could not load its current settings.");
        }
        if self.binding_dialog.open {
            return Err(
                "Finish or close the current binding editor before opening another binding.",
            );
        }
        if self.pending_delete.is_some() || self.delete_confirm_modal.is_open() {
            return Err("Resolve the pending gesture deletion before opening a binding.");
        }
        if self.rename_idx.is_some() {
            return Err("Finish the current gesture rename before opening a binding.");
        }
        let Some(gesture) = self.db.gestures.get(gesture_index) else {
            return Err("The requested gesture is no longer present.");
        };
        let Some(binding) = gesture.bindings.get(binding_index) else {
            return Err("The requested gesture binding is no longer present.");
        };
        if self
            .committed_db
            .gestures
            .get(gesture_index)
            .and_then(|gesture| gesture.bindings.get(binding_index))
            != Some(binding)
        {
            return Err("The requested gesture binding changed in the current draft.");
        }
        let binding = binding.clone();
        self.binding_dialog
            .open_edit(gesture_index, &binding, binding_index, false);
        Ok(())
    }

    fn ensure_selection(&mut self) {
        if self.selected_idx.is_none() && !self.db.gestures.is_empty() {
            self.selected_idx = self.sorted_gesture_indices().into_iter().next();
        }
        if let Some(idx) = self.selected_idx
            && let Some(gesture) = self.db.gestures.get(idx)
        {
            self.recorder.set_dir_mode(gesture.dir_mode);
            self.token_buffer = gesture.tokens.clone();
        }
    }

    fn add_gesture(&mut self) {
        let idx = self.db.gestures.len();
        self.db.gestures.push(GestureEntry {
            label: format!("Gesture {}", idx + 1),
            tokens: String::new(),
            dir_mode: DirMode::Four,
            stroke: Vec::new(),
            enabled: true,
            bindings: Vec::new(),
        });
        let token = self.allocate_identity();
        self.gesture_tokens.push(token);
        self.binding_tokens.push(Vec::new());
        self.advance_target_generation();
        self.selected_idx = Some(idx);
        self.rename_idx = Some(idx);
        self.rename_label = self.db.gestures[idx].label.clone();
        self.recorder.set_dir_mode(DirMode::Four);
        self.binding_dialog.open = false;
    }

    fn queue_gesture_delete(&mut self, idx: usize, activation_key_release_required: bool) -> bool {
        self.ensure_identity_tokens();
        let Some(entry) = self.db.gestures.get(idx) else {
            return false;
        };
        let Some(token) = self.gesture_tokens.get(idx).copied() else {
            return false;
        };
        self.delete_confirmation_instance =
            self.delete_confirmation_instance.wrapping_add(1).max(1);
        self.pending_delete = Some(PendingGestureDelete {
            idx,
            token,
            expected: entry.clone(),
            confirmation_instance: self.delete_confirmation_instance,
            activation_key_release_required,
        });
        self.delete_confirm_modal
            .open_for(DestructiveAction::DeleteGesture);
        true
    }

    fn resolve_pending_delete_index(&self, pending: &PendingGestureDelete) -> Option<usize> {
        (self.gesture_tokens.get(pending.idx) == Some(&pending.token)
            && self.db.gestures.get(pending.idx) == Some(&pending.expected))
        .then_some(pending.idx)
    }

    fn adjust_indices_after_delete(&mut self, idx: usize) {
        if let Some(selected) = self.selected_idx {
            if selected == idx {
                self.selected_idx = None;
            } else if selected > idx {
                self.selected_idx = Some(selected - 1);
            }
        }

        if let Some(rename) = self.rename_idx {
            if rename == idx {
                self.rename_idx = None;
            } else if rename > idx {
                self.rename_idx = Some(rename - 1);
            }
        }

        self.ensure_selection();
        self.binding_dialog.editor.reset();
        self.binding_dialog.open = false;
    }

    fn apply_pending_gesture_delete(&mut self) -> bool {
        let Some(pending) = self.pending_delete.take() else {
            return false;
        };
        let Some(idx) = self.resolve_pending_delete_index(&pending) else {
            return false;
        };
        self.db.gestures.remove(idx);
        if idx < self.gesture_tokens.len() {
            self.gesture_tokens.remove(idx);
        }
        if idx < self.binding_tokens.len() {
            self.binding_tokens.remove(idx);
        }
        self.advance_target_generation();
        self.adjust_indices_after_delete(idx);
        true
    }

    fn cancel_pending_gesture_delete(&mut self) {
        self.pending_delete = None;
    }

    fn save(&mut self, app: &mut LauncherApp) {
        match self.commit_candidate_with(|candidate| {
            crate::mouse_gestures::db::replace_gestures(GESTURES_FILE, candidate)
        }) {
            Ok(committed) => {
                crate::plugins::mouse_gestures::publish_committed_gesture_db(committed);
                app.dashboard_data_cache.request_refresh(
                    crate::dashboard::data_cache::DashboardRefreshRequest::Gestures,
                );
                app.search();
                app.focus_input();
            }
            Err(e) => {
                app.report_error_message(
                    "ui operation",
                    format!("Failed to save mouse gestures: {e}"),
                );
            }
        }
    }

    fn commit_candidate_with(
        &mut self,
        commit: impl FnOnce(GestureDb) -> anyhow::Result<GestureDb>,
    ) -> anyhow::Result<GestureDb> {
        match commit(self.db.clone()) {
            Ok(committed) => {
                self.committed_db = committed.clone();
                self.db = committed.clone();
                self.load_error = None;
                self.ensure_identity_tokens();
                self.advance_target_generation();
                Ok(committed)
            }
            Err(error) => {
                self.db = self.committed_db.clone();
                self.load_error = Some(format!("Failed to save mouse gestures: {error}"));
                self.reset_identity_tokens();
                Err(error)
            }
        }
    }

    fn binding_target_label(binding: &BindingEntry) -> String {
        binding.display_target()
    }

    fn bindings_ui(
        binding_dialog: &mut BindingDialog,
        ui: &mut egui::Ui,
        entry: &mut GestureEntry,
        gesture_idx: usize,
        parent_instance: u64,
        save_now: &mut bool,
        scope: Option<&JumpModeScope>,
        authorized_owner_pass: bool,
        gesture_token: u64,
        binding_tokens: &mut Vec<u64>,
        visual_order: &mut u64,
    ) {
        compact_section_heading(ui, "Bindings", None);
        ui.horizontal_wrapped(|ui| {
            let add_binding = ui.button("Add Binding");
            let add_binding = with_button_emphasis(ui, add_binding, ButtonEmphasis::Primary);
            let _ = register_mouse_gesture_target(
                scope,
                ui,
                &add_binding,
                format!("gesture/{gesture_token}/binding/add"),
                "Add binding",
                TargetKind::Button,
                visual_order,
            );
            if add_binding.clicked() && authorized_owner_pass {
                let next_idx = entry.bindings.len();
                binding_dialog.open_new(gesture_idx, next_idx, activation_key_is_held(ui.ctx()));
            }
        });
        ui.separator();

        let mut remove_idx: Option<usize> = None;
        let mut edit_request: Option<(usize, BindingEntry, bool)> = None;
        let mut reorder_request: Option<(usize, usize)> = None;
        let mut binding_enabled_changed = false;
        let binding_len = entry.bindings.len();
        egui::ScrollArea::vertical()
            .id_salt("mg_binding_list")
            .max_height(200.0)
            .show(ui, |ui| {
                if binding_len == 0 {
                    ui.label("No bindings yet.");
                }
                for idx in 0..binding_len {
                    let binding = &mut entry.bindings[idx];
                    let binding_token = binding_tokens.get(idx).copied().unwrap_or(0);
                    ui.scope_builder(
                        egui::UiBuilder::new().id(egui::Id::new((
                            "mg-binding-row",
                            parent_instance,
                            gesture_token,
                            binding_token,
                        ))),
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                let mut enabled = binding.enabled;
                                let enabled_response = ui.checkbox(&mut enabled, "");
                                let _ = register_mouse_gesture_target(
                                    scope,
                                    ui,
                                    &enabled_response,
                                    format!(
                                        "gesture/{gesture_token}/binding/{binding_token}/enabled"
                                    ),
                                    "Binding enabled",
                                    TargetKind::Checkbox,
                                    visual_order,
                                );
                                if enabled_response.changed() && authorized_owner_pass {
                                    binding.enabled = enabled;
                                    binding_enabled_changed = true;
                                }
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(&binding.label).strong());
                                    ui.label(Self::binding_target_label(binding));
                                });
                                ui.add_space(4.0);
                                let up = ui.add_enabled(idx > 0, egui::Button::new("↑"));
                                let _ = register_mouse_gesture_target(
                                    scope,
                                    ui,
                                    &up,
                                    format!("gesture/{gesture_token}/binding/{binding_token}/up"),
                                    "Move binding up",
                                    TargetKind::Button,
                                    visual_order,
                                );
                                if up.clicked() {
                                    reorder_request = Some((idx, idx - 1));
                                }
                                let down =
                                    ui.add_enabled(idx + 1 < binding_len, egui::Button::new("↓"));
                                let _ = register_mouse_gesture_target(
                                    scope,
                                    ui,
                                    &down,
                                    format!("gesture/{gesture_token}/binding/{binding_token}/down"),
                                    "Move binding down",
                                    TargetKind::Button,
                                    visual_order,
                                );
                                if down.clicked() {
                                    reorder_request = Some((idx, idx + 1));
                                }
                                let edit = ui.button("Edit");
                                let _ = register_mouse_gesture_target(
                                    scope,
                                    ui,
                                    &edit,
                                    format!("gesture/{gesture_token}/binding/{binding_token}/edit"),
                                    "Edit binding",
                                    TargetKind::Button,
                                    visual_order,
                                );
                                if edit.clicked() {
                                    edit_request = Some((
                                        idx,
                                        binding.clone(),
                                        activation_key_is_held(ui.ctx()),
                                    ));
                                }
                                let remove = ui.button("Remove");
                                let remove =
                                    with_button_emphasis(ui, remove, ButtonEmphasis::Destructive);
                                let _ = register_mouse_gesture_target(
                                    scope,
                                    ui,
                                    &remove,
                                    format!(
                                        "gesture/{gesture_token}/binding/{binding_token}/remove"
                                    ),
                                    "Remove binding",
                                    TargetKind::Button,
                                    visual_order,
                                );
                                if remove.clicked() {
                                    remove_idx = Some(idx);
                                }
                            });
                        },
                    );
                    ui.separator();
                }
            });

        if binding_enabled_changed && authorized_owner_pass {
            *save_now = true;
        }

        if authorized_owner_pass
            && let Some((from, to)) = reorder_request
            && from < entry.bindings.len()
            && to < entry.bindings.len()
        {
            entry.bindings.swap(from, to);
            if from < binding_tokens.len() && to < binding_tokens.len() {
                binding_tokens.swap(from, to);
            }
            *save_now = true;
        }

        if authorized_owner_pass && let Some((idx, binding, key_release_required)) = edit_request {
            binding_dialog.open_edit(gesture_idx, &binding, idx, key_release_required);
        }
        if authorized_owner_pass && let Some(idx) = remove_idx {
            entry.bindings.remove(idx);
            if idx < binding_tokens.len() {
                binding_tokens.remove(idx);
            }
            *save_now = true;
        }
    }

    fn binding_dialog_ui(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        save_now: &mut bool,
        parent_instance: u64,
        scope: Option<&JumpModeScope>,
        native_ui_owner: bool,
        authorized_owner_pass: bool,
        visual_order: &mut u64,
    ) {
        if !self.binding_dialog.open {
            return;
        }

        let Some(gesture_idx) = self.binding_dialog.gesture_idx else {
            if authorized_owner_pass {
                self.binding_dialog.open = false;
            }
            return;
        };

        if gesture_idx >= self.db.gestures.len() {
            if authorized_owner_pass {
                self.binding_dialog.open = false;
            }
            return;
        }

        let mut open = self.binding_dialog.open;
        let mut close_requested = false;
        let binding_instance = self.binding_dialog.instance;
        let gesture_label = self.db.gestures[gesture_idx].label.clone();
        let window_id = crate::gui::jump_mode::ui::mouse_gesture_binding_layer_id(
            parent_instance,
            binding_instance,
        );
        let mut combo_popup_ids = Vec::new();
        egui::Window::new(format!("Bind Action: {gesture_label}"))
            .id(window_id.id)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut save_entry: Option<BindingEntry> = None;
                ui.add_enabled_ui(native_ui_owner, |ui| {
                    let editor = &mut self.binding_dialog.editor;
                    let prior_kind = editor.kind;
                    compact_section_heading(ui, "Binding details", None);
                    let response = labeled_form_row(ui, "Label", |ui| {
                        ui.text_edit_singleline(&mut editor.label)
                    });
                    let _ = register_mouse_gesture_target(
                        scope,
                        ui,
                        &response,
                        "binding/label",
                        "Binding label",
                        TargetKind::TextEntry,
                        visual_order,
                    );
                    if editor.focus_label && native_ui_owner {
                        response.request_focus();
                        if authorized_owner_pass {
                            editor.focus_label = false;
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Type");
                        for (kind, local_id, label) in [
                            (BindingKind::Execute, "execute", "Execute"),
                            (BindingKind::SetQuery, "query", "Set query"),
                            (
                                BindingKind::SetQueryAndShow,
                                "query-show",
                                "Set query + show",
                            ),
                            (
                                BindingKind::SetQueryAndExecute,
                                "query-execute",
                                "Set query + execute",
                            ),
                            (BindingKind::ToggleLauncher, "toggle", "Toggle launcher"),
                        ] {
                            let response = ui.radio_value(&mut editor.kind, kind, label);
                            let _ = register_mouse_gesture_target(
                                scope,
                                ui,
                                &response,
                                format!("binding/type/{local_id}"),
                                "Binding type",
                                TargetKind::Selectable,
                                visual_order,
                            );
                        }
                    });
                    if editor.kind != prior_kind && authorized_owner_pass {
                        self.binding_dialog.picker_revision =
                            self.binding_dialog.picker_revision.wrapping_add(1).max(1);
                    }
                    match editor.kind {
                        BindingKind::Execute => {
                            let response = labeled_form_row(ui, "Action", |ui| {
                                ui.text_edit_singleline(&mut editor.action)
                            });
                            let _ = register_mouse_gesture_target(
                                scope,
                                ui,
                                &response,
                                "binding/action",
                                "Action identifier",
                                TargetKind::TextEntry,
                                visual_order,
                            );
                            let response = labeled_form_row(ui, "Args", |ui| {
                                ui.text_edit_singleline(&mut editor.args)
                            });
                            let _ = register_mouse_gesture_target(
                                scope,
                                ui,
                                &response,
                                "binding/args",
                                "Action arguments",
                                TargetKind::TextEntry,
                                visual_order,
                            );
                        }
                        BindingKind::SetQuery
                        | BindingKind::SetQueryAndShow
                        | BindingKind::SetQueryAndExecute => {
                            let response = labeled_form_row(ui, "Query", |ui| {
                                ui.text_edit_singleline(&mut editor.action)
                            });
                            let _ = register_mouse_gesture_target(
                                scope,
                                ui,
                                &response,
                                "binding/query",
                                "Search query",
                                TargetKind::TextEntry,
                                visual_order,
                            );
                        }
                        BindingKind::ToggleLauncher => {
                            ui.label("No action details required for toggling the launcher.");
                        }
                    }
                    let enabled = ui.checkbox(&mut editor.enabled, "Enabled");
                    let _ = register_mouse_gesture_target(
                        scope,
                        ui,
                        &enabled,
                        "binding/enabled",
                        "Binding enabled",
                        TargetKind::Checkbox,
                        visual_order,
                    );
                    ui.separator();
                    if editor.kind != BindingKind::ToggleLauncher {
                        let picker_label = if editor.kind == BindingKind::Execute {
                            "Pick an action"
                        } else {
                            "Pick a query"
                        };
                        compact_section_heading(ui, picker_label, None);
                        let prior_plugin = editor.add_plugin.clone();
                        let category_identity = "binding/category";
                        let category_popup_scope =
                            scope.map(|scope| scope.combo_popup_scope(category_identity));
                        let category = labeled_form_row(ui, "Category", |ui| {
                            let mut plugin_names: Vec<_> =
                                app.plugins.iter().map(|p| p.name().to_string()).collect();
                            plugin_names.push("app".to_string());
                            plugin_names.sort_unstable();
                            egui::ComboBox::from_id_salt(("mg_binding_category", binding_instance))
                                .selected_text(if editor.add_plugin.is_empty() {
                                    "Select".to_string()
                                } else {
                                    editor.add_plugin.clone()
                                })
                                .show_ui(ui, |ui| {
                                    for (index, name) in plugin_names.iter().enumerate() {
                                        let choice_identity =
                                            opaque_choice_identity(&(name.as_str(), index));
                                        let response = ui.push_id(
                                            (
                                                "mg_binding_category_choice",
                                                binding_instance,
                                                choice_identity,
                                            ),
                                            |ui| {
                                                ui.selectable_value(
                                                    &mut editor.add_plugin,
                                                    name.to_string(),
                                                    name,
                                                )
                                            },
                                        );
                                        let _ = register_mouse_gesture_target(
                                            category_popup_scope.as_ref(),
                                            ui,
                                            &response.inner,
                                            format!("category/option/{choice_identity}"),
                                            "Category option",
                                            TargetKind::Selectable,
                                            visual_order,
                                        );
                                    }
                                })
                                .response
                        });
                        combo_popup_ids.push(egui::Popup::default_response_id(&category));
                        let _ = register_mouse_gesture_target(
                            scope,
                            ui,
                            &category,
                            category_identity,
                            "Action category",
                            TargetKind::ComboBox,
                            visual_order,
                        );
                        if editor.add_plugin != prior_plugin && authorized_owner_pass {
                            self.binding_dialog.picker_revision =
                                self.binding_dialog.picker_revision.wrapping_add(1).max(1);
                        }
                        let filter = labeled_form_row(ui, "Filter", |ui| {
                            ui.text_edit_singleline(&mut editor.add_filter)
                        });
                        let _ = register_mouse_gesture_target(
                            scope,
                            ui,
                            &filter,
                            "binding/filter",
                            "Action filter",
                            TargetKind::TextEntry,
                            visual_order,
                        );
                        if filter.changed() && authorized_owner_pass {
                            self.binding_dialog.picker_revision =
                                self.binding_dialog.picker_revision.wrapping_add(1).max(1);
                        }
                        let add_args = labeled_form_row(ui, "Args", |ui| {
                            ui.text_edit_singleline(&mut editor.add_args)
                        });
                        let _ = register_mouse_gesture_target(
                            scope,
                            ui,
                            &add_args,
                            "binding/add-args",
                            "Additional arguments",
                            TargetKind::TextEntry,
                            visual_order,
                        );
                        let picker_scope = scope.map(|scope| {
                            scope.child_namespace(format!(
                                "binding/picker/{}",
                                self.binding_dialog.picker_revision
                            ))
                        });
                        let filter_text = editor.add_filter.trim().to_lowercase();
                        let mut actions = if editor.add_plugin == "app" {
                            app.actions.as_ref().clone()
                        } else {
                            app.plugins
                                .iter()
                                .find(|p| p.name() == editor.add_plugin)
                                .map(|plugin| {
                                    resolve_action_source(plugin.as_ref(), &editor.add_filter)
                                })
                                .unwrap_or_default()
                        };
                        egui::ScrollArea::vertical()
                            .id_salt(("mg_binding_action_list", binding_instance))
                            .max_height(160.0)
                            .show(ui, |ui| {
                                for (index, action) in actions.drain(..).enumerate() {
                                    if !filter_text.is_empty()
                                        && !action.label.to_lowercase().contains(&filter_text)
                                        && !action.desc.to_lowercase().contains(&filter_text)
                                        && !action.action.to_lowercase().contains(&filter_text)
                                    {
                                        continue;
                                    }
                                    let choice_identity = action_choice_identity(&action, index);
                                    let mut clicked = false;
                                    let button = ui.push_id(
                                        (
                                            "mg_binding_choice",
                                            binding_instance,
                                            self.binding_dialog.picker_revision,
                                            choice_identity,
                                        ),
                                        |ui| {
                                            let button = ui.add_sized(
                                                [ui.available_width(), 0.0],
                                                egui::Button::new(format!(
                                                    "{} - {}",
                                                    action.label, action.desc
                                                ))
                                                .wrap(),
                                            );
                                            clicked = button.clicked();
                                            button
                                        },
                                    );
                                    let _ = register_mouse_gesture_target(
                                        picker_scope.as_ref(),
                                        ui,
                                        &button.inner,
                                        format!("option/{choice_identity}"),
                                        "Action option",
                                        TargetKind::Button,
                                        visual_order,
                                    );
                                    if clicked && authorized_owner_pass {
                                        let additional_args = editor.add_args.clone();
                                        apply_action_pick(editor, &action, &additional_args);
                                    }
                                }
                            });
                    }
                    ui.horizontal_wrapped(|ui| {
                        let save_button = ui.button("Save");
                        let save_button =
                            with_button_emphasis(ui, save_button, ButtonEmphasis::Primary);
                        let _ = register_mouse_gesture_target(
                            scope,
                            ui,
                            &save_button,
                            "binding/save",
                            "Save binding",
                            TargetKind::Button,
                            visual_order,
                        );
                        if save_button.clicked() && authorized_owner_pass {
                            let action_required = matches!(
                                editor.kind,
                                BindingKind::Execute
                                    | BindingKind::SetQuery
                                    | BindingKind::SetQueryAndShow
                                    | BindingKind::SetQueryAndExecute
                            );
                            if editor.label.trim().is_empty()
                                || (action_required && editor.action.trim().is_empty())
                            {
                                app.report_error_message(
                                    "ui operation",
                                    "Label and action required",
                                );
                            } else {
                                let action = if editor.kind == BindingKind::ToggleLauncher {
                                    String::new()
                                } else {
                                    editor.action.trim().to_string()
                                };
                                let args = if editor.kind == BindingKind::Execute
                                    && !editor.args.trim().is_empty()
                                {
                                    Some(editor.args.trim().to_string())
                                } else {
                                    None
                                };
                                save_entry = Some(BindingEntry {
                                    label: editor.label.trim().to_string(),
                                    kind: editor.kind,
                                    action,
                                    args,
                                    enabled: editor.enabled,
                                });
                            }
                        }
                        let cancel = ui.button("Cancel");
                        let _ = register_mouse_gesture_target(
                            scope,
                            ui,
                            &cancel,
                            "binding/cancel",
                            "Cancel binding",
                            TargetKind::Button,
                            visual_order,
                        );
                        if cancel.clicked() && authorized_owner_pass {
                            close_requested = true;
                        }
                    });
                });
                if let Some(binding_entry) = save_entry {
                    let bindings = &mut self.db.gestures[gesture_idx].bindings;
                    if let Some(edit_idx) = self.binding_dialog.editor.edit_idx {
                        if edit_idx >= bindings.len() {
                            bindings.push(binding_entry);
                        } else if let Some(binding) = bindings.get_mut(edit_idx) {
                            *binding = binding_entry;
                        }
                    }
                    *save_now = true;
                    close_requested = true;
                }
            });

        self.binding_dialog.combo_popup_ids = combo_popup_ids;
        if authorized_owner_pass && (!open || close_requested) {
            self.binding_dialog.editor.reset();
            self.binding_dialog.open = false;
        } else if authorized_owner_pass {
            self.binding_dialog.open = open;
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        self.ui_with_jump_mode(ctx, app, 0, None, true, true, true, true);
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        parent_instance: u64,
        scope: Option<&JumpModeScope>,
        native_parent_owner: bool,
        native_binding_owner: bool,
        native_delete_owner: bool,
        first_ui_pass: bool,
    ) {
        if !self.open {
            return;
        }
        self.ensure_identity_tokens();
        let child_owner = self.jump_mode_child_owner(parent_instance);
        let parent_native = native_parent_owner && child_owner.is_none();
        let binding_layer_owner = native_binding_owner
            && matches!(
                child_owner,
                Some(NavigationOwner::MouseGestureBinding { .. })
            );
        let delete_layer_owner = native_delete_owner
            && matches!(
                child_owner,
                Some(NavigationOwner::MouseGestureDelete { .. })
            );
        let binding_native =
            binding_layer_owner && !self.binding_dialog.activation_key_release_required;
        let delete_native = delete_layer_owner
            && self
                .pending_delete
                .as_ref()
                .is_some_and(|pending| !pending.activation_key_release_required);
        let authorized_parent = parent_native && first_ui_pass;
        let authorized_binding = binding_native && first_ui_pass;
        let authorized_delete = delete_native && first_ui_pass;
        let authorized_owner_pass = authorized_parent || authorized_binding || authorized_delete;
        let rollback = (!authorized_owner_pass).then(|| MouseGesturesDraftSnapshot::capture(self));
        let mut close = false;
        let mut save_now = false;
        let mut open = self.open;
        let mut visual_order = 0;
        let mut main_combo_popup_ids = Vec::new();

        egui::Window::new("Mouse Gestures")
            .default_size(egui::vec2(720.0, 420.0))
            .min_size(egui::vec2(520.0, 320.0))
            .max_size(egui::vec2(980.0, 760.0))
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.add_enabled_ui(parent_native, |ui| {
                    if let Some(error) = self.load_error.clone() {
                        semantic_status(ui, StatusTone::Error, "Gesture file", Some(&error));
                        secondary_help(
                            ui,
                            "The existing gesture file is read-only until it loads successfully.",
                        );
                        ui.horizontal_wrapped(|ui| {
                            let retry = ui.button("Retry");
                            let retry = with_button_emphasis(ui, retry, ButtonEmphasis::Primary);
                            let _ = register_mouse_gesture_target(
                                scope,
                                ui,
                                &retry,
                                "main/retry",
                                "Retry loading gestures",
                                TargetKind::Button,
                                &mut visual_order,
                            );
                            if retry.clicked() && authorized_parent {
                                self.open();
                            }
                            let close_response = ui.button("Close");
                            let _ = register_mouse_gesture_target(
                                scope,
                                ui,
                                &close_response,
                                "main/close",
                                "Close Mouse Gestures",
                                TargetKind::Button,
                                &mut visual_order,
                            );
                            close = close_response.clicked();
                        });
                        return;
                    }

                    ui.horizontal_wrapped(|ui| {
                        let add = ui.button("Add Gesture");
                        let add = with_button_emphasis(ui, add, ButtonEmphasis::Primary);
                        let _ = register_mouse_gesture_target(
                            scope,
                            ui,
                            &add,
                            "main/add-gesture",
                            "Add gesture",
                            TargetKind::Button,
                            &mut visual_order,
                        );
                        if add.clicked() && authorized_parent {
                            self.add_gesture();
                            save_now = true;
                        }
                        let close_response = ui.button("Close");
                        let _ = register_mouse_gesture_target(
                            scope,
                            ui,
                            &close_response,
                            "main/close",
                            "Close Mouse Gestures",
                            TargetKind::Button,
                            &mut visual_order,
                        );
                        if close_response.clicked() {
                            close = true;
                        }
                    });
                    ui.separator();
                    let content_height = ui.available_height();

                    ui.horizontal(|ui| {
                        let left_width = (ui.available_width() * 0.42).clamp(260.0, 380.0);
                        let (left_rect, _) = ui.allocate_exact_size(
                            egui::vec2(left_width, content_height),
                            egui::Sense::hover(),
                        );
                        let mut left_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(left_rect)
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                        );
                        let left_clip = left_rect.shrink(1.0);
                        left_ui.set_clip_rect(left_clip);
                        left_ui.set_min_width(left_width);
                        left_ui.set_min_height(content_height);
                        compact_section_heading(&mut left_ui, "Gestures", None);
                        egui::ScrollArea::vertical()
                            .id_salt("mg_gestures_list")
                            .auto_shrink([false, false])
                            .max_height(left_ui.available_height())
                            .show(&mut left_ui, |ui| {
                                ui.set_clip_rect(left_clip);
                                let mut request_delete_idx = None;
                                for idx in self.sorted_gesture_indices() {
                                    let Some(token) = self.gesture_token(idx) else {
                                        continue;
                                    };
                                    let selected = self.selected_idx == Some(idx);
                                    let entry = &mut self.db.gestures[idx];
                                    ui.scope_builder(
                                        egui::UiBuilder::new().id(egui::Id::new((
                                            "mg-gesture-row",
                                            parent_instance,
                                            token,
                                        ))),
                                        |ui| {
                                            ui.horizontal_wrapped(|ui| {
                                                let mut enabled = entry.enabled;
                                                let checkbox = ui.checkbox(&mut enabled, "");
                                                let _ = register_mouse_gesture_target(
                                                    scope,
                                                    ui,
                                                    &checkbox,
                                                    format!("gesture/{token}/enabled"),
                                                    "Gesture enabled",
                                                    TargetKind::Checkbox,
                                                    &mut visual_order,
                                                );
                                                if checkbox.changed() && authorized_parent {
                                                    entry.enabled = enabled;
                                                    save_now = true;
                                                }
                                                let select = ui.selectable_label(
                                                    selected,
                                                    format_gesture_label(entry),
                                                );
                                                let directive = register_mouse_gesture_target(
                                                    scope,
                                                    ui,
                                                    &select,
                                                    format!("gesture/{token}/select"),
                                                    "Gesture",
                                                    TargetKind::Action,
                                                    &mut visual_order,
                                                );
                                                if authorized_parent
                                                    && (select.clicked()
                                                        || directive
                                                            == Some(FocusDirective::Select))
                                                {
                                                    if directive == Some(FocusDirective::Select) {
                                                        select.request_focus();
                                                    }
                                                    self.selected_idx = Some(idx);
                                                    self.recorder.set_dir_mode(entry.dir_mode);
                                                    self.token_buffer = entry.tokens.clone();
                                                    self.binding_dialog.editor.reset();
                                                    self.binding_dialog.open = false;
                                                }
                                                let rename = ui.button("Rename");
                                                let _ = register_mouse_gesture_target(
                                                    scope,
                                                    ui,
                                                    &rename,
                                                    format!("gesture/{token}/rename"),
                                                    "Rename gesture",
                                                    TargetKind::Button,
                                                    &mut visual_order,
                                                );
                                                if rename.clicked() && authorized_parent {
                                                    self.rename_idx = Some(idx);
                                                    self.rename_label = entry.label.clone();
                                                }
                                                let delete = ui.button("Delete");
                                                let delete = with_button_emphasis(
                                                    ui,
                                                    delete,
                                                    ButtonEmphasis::Destructive,
                                                );
                                                let _ = register_mouse_gesture_target(
                                                    scope,
                                                    ui,
                                                    &delete,
                                                    format!("gesture/{token}/delete"),
                                                    "Delete gesture",
                                                    TargetKind::Button,
                                                    &mut visual_order,
                                                );
                                                if delete.clicked() && authorized_parent {
                                                    request_delete_idx = Some(idx);
                                                }
                                            });
                                            if self.rename_idx == Some(idx) {
                                                ui.group(|ui| {
                                                    compact_section_heading(
                                                        ui,
                                                        "Rename gesture",
                                                        None,
                                                    );
                                                    let label =
                                                        labeled_form_row(ui, "Label", |ui| {
                                                            ui.add_sized(
                                                                [ui.available_width(), 0.0],
                                                                egui::TextEdit::singleline(
                                                                    &mut self.rename_label,
                                                                ),
                                                            )
                                                        });
                                                    let _ = register_mouse_gesture_target(
                                                        scope,
                                                        ui,
                                                        &label,
                                                        format!("gesture/{token}/rename/label"),
                                                        "Gesture label",
                                                        TargetKind::TextEntry,
                                                        &mut visual_order,
                                                    );
                                                    ui.horizontal_wrapped(|ui| {
                                                        let save = ui.button("Save");
                                                        let save = with_button_emphasis(
                                                            ui,
                                                            save,
                                                            ButtonEmphasis::Primary,
                                                        );
                                                        let _ = register_mouse_gesture_target(
                                                            scope,
                                                            ui,
                                                            &save,
                                                            format!("gesture/{token}/rename/save"),
                                                            "Save gesture label",
                                                            TargetKind::Button,
                                                            &mut visual_order,
                                                        );
                                                        if save.clicked()
                                                            && authorized_parent
                                                            && !self.rename_label.trim().is_empty()
                                                        {
                                                            entry.label = self
                                                                .rename_label
                                                                .trim()
                                                                .to_string();
                                                            self.rename_idx = None;
                                                            save_now = true;
                                                        }
                                                        let cancel = ui.button("Cancel");
                                                        let _ = register_mouse_gesture_target(
                                                            scope,
                                                            ui,
                                                            &cancel,
                                                            format!(
                                                                "gesture/{token}/rename/cancel"
                                                            ),
                                                            "Cancel rename",
                                                            TargetKind::Button,
                                                            &mut visual_order,
                                                        );
                                                        if cancel.clicked() && authorized_parent {
                                                            self.rename_idx = None;
                                                        }
                                                    });
                                                });
                                            }
                                        },
                                    );
                                    if let Some(idx) = request_delete_idx.take() {
                                        let _ = self.queue_gesture_delete(
                                            idx,
                                            activation_key_is_held(ui.ctx()),
                                        );
                                    }
                                }
                            });
                        ui.separator();

                        let (right_rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), content_height),
                            egui::Sense::hover(),
                        );
                        let mut right_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(right_rect)
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                        );
                        right_ui.set_min_height(content_height);
                        right_ui.set_min_width(320.0);
                        right_ui.set_clip_rect(right_rect.shrink(1.0));
                        let _right_panel_scroll = egui::ScrollArea::both()
                            .id_salt("mg_right_panel")
                            .auto_shrink([false, false])
                            .show(&mut right_ui, |ui| {
                                let selected_native_identity =
                                    self.selected_idx.and_then(|idx| self.gesture_token(idx));
                                ui.push_id(
                                    ("mg-selected-gesture-detail", selected_native_identity),
                                    |ui| {
                                        if let Some(idx) = self.selected_idx
                                            && let Some(token) = self.gesture_token(idx)
                                            && let Some(entry) = self.db.gestures.get_mut(idx)
                                        {
                                            let selected_scope = scope.map(|scope| {
                                                scope.child_namespace(format!("gesture/{token}"))
                                            });
                                            compact_section_heading(ui, "Recorder", None);
                                            let combo_identity = "direction-mode";
                                            let combo_popup_scope =
                                                selected_scope.as_ref().map(|scope| {
                                                    scope.combo_popup_scope(combo_identity)
                                                });
                                            let mut dir_mode = entry.dir_mode;
                                            let combo =
                                                labeled_form_row(ui, "Direction mode", |ui| {
                                                    egui::ComboBox::from_id_salt((
                                                        "mg_dir_mode",
                                                        token,
                                                    ))
                                                    .selected_text(match dir_mode {
                                                        DirMode::Four => "4-way",
                                                        DirMode::Eight => "8-way",
                                                    })
                                                    .show_ui(ui, |ui| {
                                                        for (mode, local_id, label) in [
                                                            (DirMode::Four, "four", "4-way"),
                                                            (DirMode::Eight, "eight", "8-way"),
                                                        ] {
                                                            let response = ui.selectable_value(
                                                                &mut dir_mode,
                                                                mode,
                                                                label,
                                                            );
                                                            let _ = register_mouse_gesture_target(
                                                                combo_popup_scope.as_ref(),
                                                                ui,
                                                                &response,
                                                                format!("direction/{local_id}"),
                                                                "Direction option",
                                                                TargetKind::Selectable,
                                                                &mut visual_order,
                                                            );
                                                        }
                                                    })
                                                    .response
                                                });
                                            main_combo_popup_ids
                                                .push(egui::Popup::default_response_id(&combo));
                                            let _ = register_mouse_gesture_target(
                                                selected_scope.as_ref(),
                                                ui,
                                                &combo,
                                                combo_identity,
                                                "Direction mode",
                                                TargetKind::ComboBox,
                                                &mut visual_order,
                                            );
                                            if dir_mode != entry.dir_mode && authorized_parent {
                                                entry.dir_mode = dir_mode;
                                                self.recorder.set_dir_mode(dir_mode);
                                                save_now = true;
                                            }
                                            ui.label(format!(
                                                "Gesture tokens: {}",
                                                if entry.tokens.trim().is_empty() {
                                                    "∅"
                                                } else {
                                                    entry.tokens.as_str()
                                                }
                                            ));
                                            let recorded_tokens = self.recorder.tokens_string();
                                            ui.label(format!(
                                                "Recorded tokens: {}",
                                                if recorded_tokens.is_empty() {
                                                    "∅"
                                                } else {
                                                    recorded_tokens.as_str()
                                                }
                                            ));
                                            ui.horizontal_wrapped(|ui| {
                                                let use_recording = ui.button("Use Recording");
                                                let use_recording = with_button_emphasis(
                                                    ui,
                                                    use_recording,
                                                    ButtonEmphasis::Primary,
                                                );
                                                let _ = register_mouse_gesture_target(
                                                    selected_scope.as_ref(),
                                                    ui,
                                                    &use_recording,
                                                    "recording/use",
                                                    "Use recording",
                                                    TargetKind::Button,
                                                    &mut visual_order,
                                                );
                                                if use_recording.clicked() && authorized_parent {
                                                    let stroke = self.recorder.normalized_stroke();
                                                    apply_recording_to_entry(
                                                        entry,
                                                        &mut self.token_buffer,
                                                        &recorded_tokens,
                                                        stroke,
                                                    );
                                                    self.recorder.reset();
                                                    save_now = true;
                                                }
                                                let clear_recording = ui.button("Clear Recording");
                                                let _ = register_mouse_gesture_target(
                                                    selected_scope.as_ref(),
                                                    ui,
                                                    &clear_recording,
                                                    "recording/clear",
                                                    "Clear recording",
                                                    TargetKind::Button,
                                                    &mut visual_order,
                                                );
                                                if clear_recording.clicked() && authorized_parent {
                                                    self.recorder.reset();
                                                }
                                                if !entry.stroke.is_empty() {
                                                    let clear_saved = ui.button("Clear Saved");
                                                    let clear_saved = with_button_emphasis(
                                                        ui,
                                                        clear_saved,
                                                        ButtonEmphasis::Destructive,
                                                    );
                                                    let _ = register_mouse_gesture_target(
                                                        selected_scope.as_ref(),
                                                        ui,
                                                        &clear_saved,
                                                        "recording/clear-saved",
                                                        "Clear saved stroke",
                                                        TargetKind::Button,
                                                        &mut visual_order,
                                                    );
                                                    if clear_saved.clicked() && authorized_parent {
                                                        entry.stroke.clear();
                                                        save_now = true;
                                                    }
                                                }
                                            });
                                            let size =
                                                egui::vec2(ui.available_width().max(320.0), 260.0);
                                            let sense = if parent_native {
                                                egui::Sense::drag()
                                            } else {
                                                egui::Sense::hover()
                                            };
                                            let (rect, response) =
                                                ui.allocate_exact_size(size, sense);
                                            let painter = ui.painter_at(rect);
                                            painter.rect_stroke(
                                                rect,
                                                0.0,
                                                egui::Stroke::new(1.0_f32, egui::Color32::GRAY),
                                                egui::StrokeKind::Middle,
                                            );
                                            if response.drag_started() {
                                                self.canvas_drag_active = true;
                                                if authorized_parent {
                                                    if !entry.stroke.is_empty() {
                                                        entry.stroke.clear();
                                                        save_now = true;
                                                    }
                                                    self.recorder.reset();
                                                    if let Some(pos) =
                                                        response.interact_pointer_pos()
                                                    {
                                                        self.recorder.push_point(pos);
                                                    }
                                                }
                                            }
                                            if response.dragged() {
                                                self.canvas_drag_active = true;
                                                if authorized_parent
                                                    && let Some(pos) =
                                                        response.interact_pointer_pos()
                                                {
                                                    self.recorder.push_point(pos);
                                                }
                                            }
                                            if response.drag_stopped() {
                                                self.canvas_drag_active = false;
                                                if authorized_parent {
                                                    let recorded_tokens =
                                                        self.recorder.tokens_string();
                                                    if !recorded_tokens.is_empty() {
                                                        let stroke =
                                                            self.recorder.normalized_stroke();
                                                        apply_recording_to_entry(
                                                            entry,
                                                            &mut self.token_buffer,
                                                            &recorded_tokens,
                                                            stroke,
                                                        );
                                                        save_now = true;
                                                    }
                                                    self.recorder.reset();
                                                }
                                            }
                                            if entry.stroke.len() >= 2 {
                                                let points =
                                                    stroke_points_in_rect(&entry.stroke, rect);
                                                if points.len() >= 2 {
                                                    painter.add(egui::Shape::line(
                                                        points,
                                                        egui::Stroke::new(
                                                            2.0_f32,
                                                            egui::Color32::from_gray(140),
                                                        ),
                                                    ));
                                                }
                                            }
                                            if self.recorder.points().len() >= 2 {
                                                painter.add(egui::Shape::line(
                                                    self.recorder.points().to_vec(),
                                                    egui::Stroke::new(
                                                        2.0_f32,
                                                        egui::Color32::LIGHT_BLUE,
                                                    ),
                                                ));
                                            }
                                            ui.separator();
                                            let token_response =
                                                labeled_form_row(ui, "Tokens", |ui| {
                                                    ui.add(
                                                        egui::TextEdit::singleline(
                                                            &mut self.token_buffer,
                                                        )
                                                        .id_salt(("mg_tokens", token)),
                                                    )
                                                });
                                            let _ = register_mouse_gesture_target(
                                                selected_scope.as_ref(),
                                                ui,
                                                &token_response,
                                                "tokens/input",
                                                "Gesture tokens",
                                                TargetKind::TextEntry,
                                                &mut visual_order,
                                            );
                                            ui.horizontal_wrapped(|ui| {
                                                let import = ui.button("Import");
                                                let _ = register_mouse_gesture_target(
                                                    selected_scope.as_ref(),
                                                    ui,
                                                    &import,
                                                    "tokens/import",
                                                    "Import tokens",
                                                    TargetKind::Button,
                                                    &mut visual_order,
                                                );
                                                if import.clicked() && authorized_parent {
                                                    entry.tokens =
                                                        self.token_buffer.trim().to_string();
                                                    save_now = true;
                                                }
                                                let export = ui.button("Export");
                                                let _ = register_mouse_gesture_target(
                                                    selected_scope.as_ref(),
                                                    ui,
                                                    &export,
                                                    "tokens/export",
                                                    "Copy tokens",
                                                    TargetKind::Button,
                                                    &mut visual_order,
                                                );
                                                if export.clicked() && authorized_parent {
                                                    self.token_buffer = entry.tokens.clone();
                                                    ctx.output_mut(|output| {
                                                        output.commands.push(
                                                            egui::OutputCommand::CopyText(
                                                                self.token_buffer.clone(),
                                                            ),
                                                        );
                                                    });
                                                }
                                            });
                                            ui.separator();
                                            let binding_tokens = self.binding_tokens.get_mut(idx);
                                            if let Some(binding_tokens) = binding_tokens {
                                                Self::bindings_ui(
                                                    &mut self.binding_dialog,
                                                    ui,
                                                    entry,
                                                    idx,
                                                    parent_instance,
                                                    &mut save_now,
                                                    selected_scope.as_ref(),
                                                    authorized_parent,
                                                    token,
                                                    binding_tokens,
                                                    &mut visual_order,
                                                );
                                            }
                                        } else {
                                            secondary_help(ui, "Select a gesture to edit.");
                                        }
                                    },
                                );
                            });
                        #[cfg(test)]
                        {
                            self.last_right_panel_scroll_area_id = Some(_right_panel_scroll.id);
                            self.last_right_panel_scroll_max_offset_y =
                                (_right_panel_scroll.content_size.y
                                    - _right_panel_scroll.inner_rect.height())
                                .max(0.0);
                        }
                    });
                });
            });

        self.main_combo_popup_ids = main_combo_popup_ids;
        if self.load_error.is_none() {
            self.binding_dialog_ui(
                ctx,
                app,
                &mut save_now,
                parent_instance,
                scope,
                binding_native,
                authorized_binding,
                &mut visual_order,
            );
            let delete_result = self
                .pending_delete
                .as_ref()
                .map(|pending| {
                    self.delete_confirm_modal.ui_with_jump_mode(
                        ctx,
                        crate::gui::jump_mode::ui::mouse_gesture_delete_layer_id(
                            parent_instance,
                            pending.confirmation_instance,
                        )
                        .id,
                        scope,
                        delete_native,
                        authorized_delete,
                    )
                })
                .unwrap_or(ConfirmationResult::None);
            match delete_result {
                ConfirmationResult::Confirmed if authorized_delete => {
                    if self.apply_pending_gesture_delete() {
                        save_now = true;
                    }
                }
                ConfirmationResult::Cancelled if authorized_delete => {
                    self.cancel_pending_gesture_delete();
                }
                _ => {}
            }
            if save_now && (authorized_parent || authorized_binding || authorized_delete) {
                self.save(app);
            }
        }
        if authorized_parent {
            if close || !open {
                self.canvas_drag_active = false;
                self.open = false;
            } else {
                self.open = open;
            }
        }
        self.ensure_identity_tokens();
        if let Some(snapshot) = rollback {
            let drag_active = self.canvas_drag_active;
            let main_popup_ids = self.main_combo_popup_ids.clone();
            let binding_popup_ids = self.binding_dialog.combo_popup_ids.clone();
            snapshot.restore(self);
            self.canvas_drag_active = drag_active;
            self.main_combo_popup_ids = main_popup_ids;
            self.binding_dialog.combo_popup_ids = binding_popup_ids;
        }
        let activation_key_held =
            ctx.input(|input| input.key_down(egui::Key::Enter) || input.key_down(egui::Key::Space));
        // If a child was opened by Enter/Space, keep its controls disabled until that
        // activation key has actually gone up while the exact child owns the window.
        // The release frame only clears this guard; a later distinct press can act.
        if binding_layer_owner
            && self.binding_dialog.activation_key_release_required
            && !activation_key_held
        {
            self.binding_dialog.activation_key_release_required = false;
        }
        if delete_layer_owner
            && !activation_key_held
            && let Some(pending) = self.pending_delete.as_mut()
        {
            pending.activation_key_release_required = false;
        }
    }
}

fn hotkey_directory_binding_is_current(
    source: crate::hotkey::directory::SourceSnapshot,
    publication: u64,
    entry_id: &crate::hotkey::directory::EntryId,
    gesture_index: usize,
    binding_index: usize,
) -> bool {
    use crate::hotkey::directory::{EditDestination, Source};

    let (Ok(gesture_index), Ok(binding_index)) =
        (u32::try_from(gesture_index), u32::try_from(binding_index))
    else {
        return false;
    };
    source.source == Source::MouseGestures
        && source.revision == publication
        && source.candidates.iter().any(|candidate| {
            candidate.owner == crate::hotkey::directory::Owner::MouseGestures
                && candidate.id == *entry_id
                && candidate.edit.as_ref()
                    == Some(&EditDestination::GestureBinding {
                        publication,
                        gesture_index,
                        binding_index,
                    })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn published_hotkey_binding(
        publication: u64,
        gesture_index: u32,
        binding_index: u32,
    ) -> crate::hotkey::directory::SourceSnapshot {
        use crate::hotkey::directory::{
            Candidate, CandidateState, EditDestination, EntryId, Owner, RouteKind, Scope, Source,
            SourceSnapshot,
        };

        SourceSnapshot::new(
            Source::MouseGestures,
            publication,
            vec![Candidate {
                id: EntryId::new(format!(
                    "gesture:{publication}:{gesture_index}:{binding_index}"
                )),
                owner: Owner::MouseGestures,
                owner_label: "Mouse Gestures".into(),
                action_label: "Launch — binding 1".into(),
                chord: None,
                route: RouteKind::Gesture,
                scope: Scope::Gesture,
                source_revision: publication,
                expected_runtime_generation: None,
                state: CandidateState::Configured,
                reason: None,
                edit: Some(EditDestination::GestureBinding {
                    publication,
                    gesture_index,
                    binding_index,
                }),
            }],
            Vec::new(),
        )
    }

    fn owner_database_matches(
        live_publication: u64,
        live_database: &GestureDb,
        publication: u64,
        entry_id: &crate::hotkey::directory::EntryId,
        gesture_index: usize,
        binding_index: usize,
        committed_database: &GestureDb,
    ) -> bool {
        let expected_id = format!("gesture:{publication}:{gesture_index}:{binding_index}");
        if live_publication != publication || entry_id.as_str() != expected_id.as_str() {
            return false;
        }
        let (Some(live_gesture), Some(committed_gesture)) = (
            live_database.gestures.get(gesture_index),
            committed_database.gestures.get(gesture_index),
        ) else {
            return false;
        };
        live_gesture == committed_gesture && live_gesture.bindings.get(binding_index).is_some()
    }

    fn gesture(label: &str, tokens: &str) -> GestureEntry {
        GestureEntry {
            label: label.into(),
            tokens: tokens.into(),
            dir_mode: DirMode::Four,
            stroke: Vec::new(),
            enabled: true,
            bindings: Vec::new(),
        }
    }

    #[test]
    fn gesture_delete_is_queued_until_confirmed() {
        let mut dlg = MgGesturesDialog::default();
        dlg.db.gestures = vec![gesture("A", "R")];
        dlg.selected_idx = Some(0);

        assert!(dlg.queue_gesture_delete(0, false));
        assert_eq!(dlg.db.gestures.len(), 1);
        assert!(dlg.pending_delete.is_some());
    }

    #[test]
    fn hotkey_directory_ui_reveals_existing_gesture_binding_without_replacing_draft() {
        let committed = GestureDb {
            schema_version: crate::mouse_gestures::db::SCHEMA_VERSION,
            gestures: vec![
                GestureEntry {
                    bindings: vec![BindingEntry {
                        label: "Launch current query".into(),
                        kind: BindingKind::Execute,
                        action: "launcher:show".into(),
                        args: None,
                        enabled: true,
                    }],
                    ..gesture("Launch", "R")
                },
                gesture("Working draft", "L"),
            ],
        };
        let mut draft = committed.clone();
        draft.gestures[1].label = "Unsaved gesture label".into();

        let mut dialog = MgGesturesDialog::default();
        dialog.open = true;
        dialog.db = draft.clone();
        dialog.committed_db = committed.clone();
        dialog.selected_idx = Some(1);
        dialog.token_buffer = "unfinished token draft".into();

        let publication = 17;
        let entry_id = crate::hotkey::directory::EntryId::new("gesture:17:0:0");
        let live_source = published_hotkey_binding(publication, 0, 0);
        let live_database = committed.clone();
        dialog
            .reveal_binding_with_hotkey_directory_source(
                publication,
                &entry_id,
                0,
                0,
                || live_source.clone(),
                |publication, entry_id, gesture_index, binding_index, baseline| {
                    owner_database_matches(
                        publication,
                        &live_database,
                        publication,
                        entry_id,
                        gesture_index,
                        binding_index,
                        baseline,
                    )
                },
                || unreachable!("an already-open editor does not reload its file"),
            )
            .expect("the exact unchanged binding remains a safe destination");

        assert!(dialog.binding_dialog.open);
        assert_eq!(dialog.binding_dialog.gesture_idx, Some(0));
        assert_eq!(dialog.binding_dialog.editor.edit_idx, Some(0));
        assert_eq!(dialog.binding_dialog.editor.label, "Launch current query");
        assert_eq!(dialog.db, draft, "the unsaved database draft is retained");
        assert_eq!(dialog.committed_db, committed);
        assert_eq!(dialog.selected_idx, Some(1));
        assert_eq!(dialog.token_buffer, "unfinished token draft");

        let mut changed_target_draft = draft.clone();
        changed_target_draft.gestures[0].bindings[0].label = "Changed target draft".into();
        let mut stale_dialog = MgGesturesDialog::default();
        stale_dialog.open = true;
        stale_dialog.db = changed_target_draft.clone();
        stale_dialog.committed_db = committed.clone();
        stale_dialog.selected_idx = Some(1);
        stale_dialog.token_buffer = "keep this draft".into();
        assert_eq!(
            stale_dialog.reveal_binding_with_hotkey_directory_source(
                publication,
                &entry_id,
                0,
                0,
                || live_source.clone(),
                |publication, entry_id, gesture_index, binding_index, baseline| {
                    owner_database_matches(
                        publication,
                        &committed,
                        publication,
                        entry_id,
                        gesture_index,
                        binding_index,
                        baseline,
                    )
                },
                || unreachable!("an already-open editor does not reload its file"),
            ),
            Err("The requested gesture binding changed in the current draft.")
        );
        assert!(!stale_dialog.binding_dialog.open);
        assert_eq!(stale_dialog.db, changed_target_draft);
        assert_eq!(stale_dialog.selected_idx, Some(1));
        assert_eq!(stale_dialog.token_buffer, "keep this draft");

        let newer_publication = published_hotkey_binding(publication + 1, 0, 0);
        let mut publication_stale_dialog = MgGesturesDialog::default();
        publication_stale_dialog.open = true;
        publication_stale_dialog.db = draft.clone();
        publication_stale_dialog.committed_db = committed.clone();
        publication_stale_dialog.selected_idx = Some(1);
        publication_stale_dialog.token_buffer = "preserve on publication change".into();
        assert_eq!(
            publication_stale_dialog.reveal_binding_with_hotkey_directory_source(
                publication,
                &entry_id,
                0,
                0,
                || newer_publication.clone(),
                |publication, entry_id, gesture_index, binding_index, baseline| {
                    owner_database_matches(
                        publication,
                        &committed,
                        publication,
                        entry_id,
                        gesture_index,
                        binding_index,
                        baseline,
                    )
                },
                || unreachable!("an already-open editor does not reload its file"),
            ),
            Err("The requested gesture binding changed after this snapshot.")
        );
        assert!(!publication_stale_dialog.binding_dialog.open);
        assert_eq!(publication_stale_dialog.db, draft);
        assert_eq!(publication_stale_dialog.committed_db, committed);
        assert_eq!(publication_stale_dialog.selected_idx, Some(1));
        assert_eq!(
            publication_stale_dialog.token_buffer,
            "preserve on publication change"
        );

        let mut changed_live_database = committed.clone();
        changed_live_database.gestures[0].bindings[0].label = "Current runtime binding".into();
        let mut stale_baseline_dialog = MgGesturesDialog::default();
        stale_baseline_dialog.open = true;
        stale_baseline_dialog.db = draft.clone();
        stale_baseline_dialog.committed_db = committed.clone();
        stale_baseline_dialog.selected_idx = Some(1);
        stale_baseline_dialog.token_buffer = "preserve stale-baseline draft".into();
        assert_eq!(
            stale_baseline_dialog.reveal_binding_with_hotkey_directory_source(
                publication,
                &entry_id,
                0,
                0,
                || live_source.clone(),
                |publication, entry_id, gesture_index, binding_index, baseline| {
                    owner_database_matches(
                        publication,
                        &changed_live_database,
                        publication,
                        entry_id,
                        gesture_index,
                        binding_index,
                        baseline,
                    )
                },
                || unreachable!("an already-open editor does not reload its file"),
            ),
            Err("The requested gesture binding changed in the current owner database.")
        );
        assert!(!stale_baseline_dialog.binding_dialog.open);
        assert_eq!(stale_baseline_dialog.db, draft);
        assert_eq!(stale_baseline_dialog.committed_db, committed);
        assert_eq!(stale_baseline_dialog.selected_idx, Some(1));
        assert_eq!(
            stale_baseline_dialog.token_buffer,
            "preserve stale-baseline draft"
        );

        let mut closed_stale_dialog = MgGesturesDialog::default();
        closed_stale_dialog.db = draft.clone();
        closed_stale_dialog.committed_db = committed.clone();
        closed_stale_dialog.selected_idx = Some(1);
        closed_stale_dialog.token_buffer = "preserve closed draft state".into();
        assert_eq!(
            closed_stale_dialog.reveal_binding_with_hotkey_directory_source(
                publication,
                &entry_id,
                0,
                0,
                || live_source.clone(),
                |publication, entry_id, gesture_index, binding_index, database| {
                    owner_database_matches(
                        publication,
                        &committed,
                        publication,
                        entry_id,
                        gesture_index,
                        binding_index,
                        database,
                    )
                },
                || Ok(changed_live_database.clone()),
            ),
            Err("The requested gesture binding changed in the current owner database.")
        );
        assert!(!closed_stale_dialog.open);
        assert_eq!(closed_stale_dialog.db, draft);
        assert_eq!(closed_stale_dialog.committed_db, committed);
        assert_eq!(closed_stale_dialog.selected_idx, Some(1));
        assert_eq!(
            closed_stale_dialog.token_buffer,
            "preserve closed draft state"
        );

        let mut closed_current_dialog = MgGesturesDialog::default();
        closed_current_dialog.db = draft;
        closed_current_dialog.committed_db = committed.clone();
        assert_eq!(
            closed_current_dialog.reveal_binding_with_hotkey_directory_source(
                publication,
                &entry_id,
                0,
                0,
                || live_source.clone(),
                |publication, entry_id, gesture_index, binding_index, database| {
                    owner_database_matches(
                        publication,
                        &committed,
                        publication,
                        entry_id,
                        gesture_index,
                        binding_index,
                        database,
                    )
                },
                || Ok(committed.clone()),
            ),
            Ok(())
        );
        assert!(closed_current_dialog.open);
        assert!(closed_current_dialog.binding_dialog.open);
        assert_eq!(closed_current_dialog.db, committed);
    }

    #[test]
    fn cancelling_gesture_delete_keeps_db_and_selection() {
        let mut dlg = MgGesturesDialog::default();
        dlg.db.gestures = vec![gesture("A", "R"), gesture("B", "L")];
        dlg.selected_idx = Some(1);
        dlg.rename_idx = Some(1);

        assert!(dlg.queue_gesture_delete(1, false));
        dlg.cancel_pending_gesture_delete();

        assert_eq!(dlg.db.gestures.len(), 2);
        assert_eq!(dlg.selected_idx, Some(1));
        assert_eq!(dlg.rename_idx, Some(1));
    }

    #[test]
    fn confirmed_gesture_delete_adjusts_indices() {
        let mut dlg = MgGesturesDialog::default();
        dlg.db.gestures = vec![gesture("A", "R"), gesture("B", "L"), gesture("C", "U")];
        dlg.selected_idx = Some(2);
        dlg.rename_idx = Some(2);

        assert!(dlg.queue_gesture_delete(1, false));
        assert!(dlg.apply_pending_gesture_delete());

        assert_eq!(dlg.db.gestures.len(), 2);
        assert_eq!(dlg.selected_idx, Some(1));
        assert_eq!(dlg.rename_idx, Some(1));
    }

    #[test]
    fn load_failure_keeps_last_good_document_read_only_until_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mouse_gestures.json");
        let mut dlg = MgGesturesDialog::default();
        crate::common::persistence::save_json_atomic(
            &path,
            &GestureDb {
                schema_version: crate::mouse_gestures::db::SCHEMA_VERSION,
                gestures: vec![gesture("last-good", "L")],
            },
        )
        .unwrap();
        dlg.reload_from(&path);
        std::fs::write(&path, b"{broken").unwrap();

        dlg.reload_from(&path);
        assert!(dlg.load_error.is_some());
        assert_eq!(dlg.db.gestures[0].label, "last-good");

        crate::common::persistence::save_json_atomic(
            &path,
            &GestureDb {
                schema_version: crate::mouse_gestures::db::SCHEMA_VERSION,
                gestures: vec![gesture("recovered", "R")],
            },
        )
        .unwrap();
        dlg.reload_from(&path);
        assert!(dlg.load_error.is_none());
        assert_eq!(dlg.db.gestures[0].label, "recovered");
    }

    #[test]
    fn save_failure_restores_committed_dialog_snapshot_and_retains_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mouse_gestures.json");
        let committed = GestureDb {
            schema_version: crate::mouse_gestures::db::SCHEMA_VERSION,
            gestures: vec![gesture("committed", "L")],
        };
        crate::common::persistence::save_json_atomic(&path, &committed).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let mut dialog = MgGesturesDialog::default();
        dialog.reload_from(&path);
        dialog.db.gestures[0].label = "uncommitted".into();

        let result = dialog.commit_candidate_with(|_| anyhow::bail!("injected save failure"));

        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(dialog.db, committed);
        assert_eq!(dialog.committed_db, committed);
        assert!(
            dialog
                .load_error
                .as_deref()
                .unwrap()
                .contains("injected save failure")
        );
    }

    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockPlugin {
        name: String,
        search_results_by_query: HashMap<String, Vec<crate::actions::Action>>,
        command_results: Vec<crate::actions::Action>,
        seen_queries: Mutex<Vec<String>>,
    }

    impl MockPlugin {
        fn with_query_results(
            name: &str,
            search_results_by_query: impl IntoIterator<Item = (String, Vec<crate::actions::Action>)>,
            command_results: Vec<crate::actions::Action>,
        ) -> Self {
            Self {
                name: name.into(),
                search_results_by_query: search_results_by_query.into_iter().collect(),
                command_results,
                seen_queries: Mutex::new(Vec::new()),
            }
        }

        fn seen_queries(&self) -> Vec<String> {
            self.seen_queries
                .lock()
                .expect("seen_queries lock poisoned")
                .clone()
        }
    }

    impl crate::plugin::Plugin for MockPlugin {
        fn search(&self, query: &str) -> Vec<crate::actions::Action> {
            self.seen_queries
                .lock()
                .expect("seen_queries lock poisoned")
                .push(query.to_string());
            self.search_results_by_query
                .get(query)
                .cloned()
                .unwrap_or_default()
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "mock"
        }

        fn capabilities(&self) -> &[&str] {
            &[]
        }

        fn commands(&self) -> Vec<crate::actions::Action> {
            self.command_results.clone()
        }
    }

    #[test]
    fn auto_label_is_applied_only_for_default_generated_labels() {
        let mut entry = gesture("Gesture 3", "");

        let mut token_buffer = String::new();
        apply_recording_to_entry(&mut entry, &mut token_buffer, "udlr", Vec::new());

        assert_eq!(entry.tokens, "udlr");
        assert_eq!(entry.label, "Gesture UDLR");
    }

    #[test]
    fn customized_label_is_preserved_when_new_recording_is_applied() {
        let mut entry = gesture("My Favorite Gesture", "UD");

        let mut token_buffer = String::new();
        apply_recording_to_entry(&mut entry, &mut token_buffer, "LR", Vec::new());

        assert_eq!(entry.tokens, "LR");
        assert_eq!(entry.label, "My Favorite Gesture");
    }

    fn query_result(
        query: &str,
        actions: Vec<crate::actions::Action>,
    ) -> (String, Vec<crate::actions::Action>) {
        (query.to_string(), actions)
    }
    fn concrete_action(action: &str) -> crate::actions::Action {
        crate::actions::Action {
            label: "Concrete".into(),
            desc: "Mock".into(),
            action: action.into(),
            args: None,
        }
    }

    fn query_action(action: &str) -> crate::actions::Action {
        crate::actions::Action {
            label: "Query".into(),
            desc: "Mock".into(),
            action: action.into(),
            args: None,
        }
    }

    #[test]
    fn resolve_action_source_prefers_mapped_list_search_and_falls_back_to_commands() {
        let command_action = query_action("query:cb list");
        let mapped_plugin = MockPlugin::with_query_results(
            "clipboard",
            [query_result(
                "cb list abc",
                vec![concrete_action("clipboard:copy:1")],
            )],
            vec![command_action.clone()],
        );

        let mapped_actions = resolve_action_source(&mapped_plugin, "abc");
        assert_eq!(mapped_actions.len(), 1);
        assert_eq!(mapped_actions[0].action, "clipboard:copy:1");
        assert_eq!(mapped_plugin.seen_queries(), vec!["cb list abc"]);

        let fallback_plugin = MockPlugin::with_query_results(
            "custom",
            [query_result(
                "ignored",
                vec![concrete_action("clipboard:copy:2")],
            )],
            vec![command_action.clone()],
        );
        let fallback_actions = resolve_action_source(&fallback_plugin, "abc");
        assert_eq!(fallback_actions.len(), 1);
        assert_eq!(fallback_actions[0].action, command_action.action);
        assert!(fallback_plugin.seen_queries().is_empty());
    }

    #[test]
    fn resolve_action_source_uses_mapped_search_for_each_supported_list_plugin() {
        let cases = [
            ("clipboard", "cb list", "clipboard:copy:0"),
            ("emoji", "emoji list", "clipboard:😀"),
            ("processes", "ps", "process:switch:42"),
            ("note", "note list", "note:open:alpha"),
            ("notes", "note list", "note:open:beta"),
            ("folders", "f list", "/tmp"),
            ("bookmarks", "bm list", "https://example.com"),
            ("todo", "todo list", "todo:toggle:1"),
            ("snippets", "cs list", "snippet:copy:1"),
        ];

        for (plugin_name, expected_query_prefix, expected_action) in cases {
            let plugin = MockPlugin::with_query_results(
                plugin_name,
                [query_result(
                    &format!("{expected_query_prefix} abc"),
                    vec![concrete_action(expected_action)],
                )],
                vec![query_action("query:fallback")],
            );

            let actions = resolve_action_source(&plugin, "abc");
            assert_eq!(actions.len(), 1, "plugin={plugin_name}");
            assert_eq!(actions[0].action, expected_action, "plugin={plugin_name}");
            assert_eq!(
                plugin.seen_queries(),
                vec![format!("{expected_query_prefix} abc")],
                "plugin={plugin_name}"
            );
        }
    }

    #[test]
    fn resolve_action_source_falls_back_to_commands_when_mapped_search_is_empty() {
        let command_action = query_action("query:note list");
        let plugin = MockPlugin::with_query_results(
            "note",
            Vec::<(String, Vec<crate::actions::Action>)>::new(),
            vec![command_action.clone()],
        );

        let actions = resolve_action_source(&plugin, "abc");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, command_action.action);
        assert_eq!(
            plugin.seen_queries(),
            vec!["note list abc", "note search abc"]
        );
    }

    #[test]
    fn resolve_action_source_retries_alternate_prefix_when_primary_only_returns_query_actions() {
        let plugin = MockPlugin::with_query_results(
            "clipboard",
            [
                query_result(
                    "cb list abc",
                    vec![query_action("query:cb list"), query_action("query:cb")],
                ),
                query_result("cb abc", vec![concrete_action("clipboard:copy:7")]),
            ],
            vec![query_action("query:cb list")],
        );

        let actions = resolve_action_source(&plugin, "abc");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "clipboard:copy:7");
        assert_eq!(plugin.seen_queries(), vec!["cb list abc", "cb abc"]);
    }

    #[test]
    fn resolve_action_source_uses_commands_for_unmapped_plugins() {
        let command_action = query_action("query:custom");
        let plugin = MockPlugin::with_query_results(
            "custom",
            [query_result(
                "custom abc",
                vec![concrete_action("custom:run")],
            )],
            vec![command_action.clone()],
        );

        let actions = resolve_action_source(&plugin, "abc");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, command_action.action);
        assert!(plugin.seen_queries().is_empty());
    }
}
