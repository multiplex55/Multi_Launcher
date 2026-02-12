use crate::gui::confirmation_modal::{ConfirmationModal, ConfirmationResult, DestructiveAction};
use crate::gui::LauncherApp;
use crate::mouse_gestures::db::{
    format_gesture_label, load_gestures, save_gestures, BindingEntry, BindingKind, GestureDb,
    GestureEntry, GESTURES_FILE,
};
use crate::mouse_gestures::engine::{DirMode, GestureTracker};
use crate::mouse_gestures::service::MouseGestureConfig;
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
    if sampled.last().copied() != points.last().copied() {
        if let Some(last) = points.last().copied() {
            sampled.push(last);
        }
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

#[derive(Default)]
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
    gesture_idx: Option<usize>,
    editor: BindingEditor,
}

impl BindingDialog {
    fn open_new(&mut self, gesture_idx: usize, next_idx: usize) {
        self.open = true;
        self.gesture_idx = Some(gesture_idx);
        self.editor.start_edit(None, next_idx);
    }

    fn open_edit(&mut self, gesture_idx: usize, binding: &BindingEntry, edit_idx: usize) {
        self.open = true;
        self.gesture_idx = Some(gesture_idx);
        self.editor.start_edit(Some(binding), edit_idx);
    }
}

pub struct MgGesturesDialog {
    pub open: bool,
    db: GestureDb,
    selected_idx: Option<usize>,
    rename_idx: Option<usize>,
    rename_label: String,
    recorder: GestureRecorder,
    token_buffer: String,
    binding_dialog: BindingDialog,
    delete_confirm_modal: ConfirmationModal,
    pending_delete: Option<PendingGestureDelete>,
}

#[derive(Debug, Clone)]
struct PendingGestureDelete {
    idx: usize,
    label: String,
    tokens: String,
}

impl Default for MgGesturesDialog {
    fn default() -> Self {
        let config = default_recorder_config();
        Self {
            open: false,
            db: GestureDb::default(),
            selected_idx: None,
            rename_idx: None,
            rename_label: String::new(),
            recorder: GestureRecorder::new(DirMode::Four, config),
            token_buffer: String::new(),
            binding_dialog: BindingDialog::default(),
            delete_confirm_modal: ConfirmationModal::default(),
            pending_delete: None,
        }
    }
}

impl MgGesturesDialog {
    /// Returns gesture indices sorted by label (case-insensitive) for display purposes.
    fn sorted_gesture_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.db.gestures.len()).collect();
        indices.sort_by_key(|&i| self.db.gestures[i].label.to_lowercase());
        indices
    }

    pub fn open(&mut self) {
        self.db = load_gestures(GESTURES_FILE).unwrap_or_default();
        self.open = true;
        self.selected_idx = self.sorted_gesture_indices().into_iter().next();
        self.rename_idx = None;
        self.rename_label.clear();
        self.token_buffer.clear();
        self.binding_dialog.open = false;
        self.ensure_selection();
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
        self.add_gesture();
    }

    pub fn open_binding_editor(&mut self) {
        self.open();
        if self.db.gestures.is_empty() {
            self.add_gesture();
        } else {
            self.ensure_selection();
        }
        if let Some(idx) = self.selected_idx {
            let next_idx = self.db.gestures[idx].bindings.len();
            self.binding_dialog.open_new(idx, next_idx);
        }
    }

    fn ensure_selection(&mut self) {
        if self.selected_idx.is_none() && !self.db.gestures.is_empty() {
            self.selected_idx = self.sorted_gesture_indices().into_iter().next();
        }
        if let Some(idx) = self.selected_idx {
            if let Some(gesture) = self.db.gestures.get(idx) {
                self.recorder.set_dir_mode(gesture.dir_mode);
                self.token_buffer = gesture.tokens.clone();
            }
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
        self.selected_idx = Some(idx);
        self.rename_idx = Some(idx);
        self.rename_label = self.db.gestures[idx].label.clone();
        self.recorder.set_dir_mode(DirMode::Four);
        self.binding_dialog.open = false;
    }

    fn queue_gesture_delete(&mut self, idx: usize) -> bool {
        let Some(entry) = self.db.gestures.get(idx) else {
            return false;
        };
        self.pending_delete = Some(PendingGestureDelete {
            idx,
            label: entry.label.clone(),
            tokens: entry.tokens.clone(),
        });
        self.delete_confirm_modal
            .open_for(DestructiveAction::DeleteGesture);
        true
    }

    fn resolve_pending_delete_index(&self, pending: &PendingGestureDelete) -> Option<usize> {
        if self
            .db
            .gestures
            .get(pending.idx)
            .is_some_and(|g| g.label == pending.label && g.tokens == pending.tokens)
        {
            return Some(pending.idx);
        }
        self.db
            .gestures
            .iter()
            .position(|g| g.label == pending.label && g.tokens == pending.tokens)
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
        self.adjust_indices_after_delete(idx);
        true
    }

    fn cancel_pending_gesture_delete(&mut self) {
        self.pending_delete = None;
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_gestures(GESTURES_FILE, &self.db) {
            app.set_error(format!("Failed to save mouse gestures: {e}"));
        } else {
            app.search();
            app.focus_input();
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
        save_now: &mut bool,
    ) {
        ui.label("Bindings");
        ui.horizontal(|ui| {
            if ui.button("Add Binding").clicked() {
                let next_idx = entry.bindings.len();
                binding_dialog.open_new(gesture_idx, next_idx);
            }
        });
        ui.separator();

        let mut remove_idx: Option<usize> = None;
        let mut edit_request: Option<(usize, BindingEntry)> = None;
        let mut reorder_request: Option<(usize, usize)> = None;
        let mut binding_enabled_changed = false;
        let binding_len = entry.bindings.len();
        egui::ScrollArea::vertical()
            .id_source("mg_binding_list")
            .max_height(200.0)
            .show(ui, |ui| {
                if binding_len == 0 {
                    ui.label("No bindings yet.");
                }
                for idx in 0..binding_len {
                    let binding = &mut entry.bindings[idx];
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut binding.enabled, "").changed() {
                            binding_enabled_changed = true;
                        }
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(&binding.label).strong());
                            ui.label(Self::binding_target_label(binding));
                        });
                        ui.add_space(4.0);
                        if ui.add_enabled(idx > 0, egui::Button::new("↑")).clicked() {
                            reorder_request = Some((idx, idx - 1));
                        }
                        if ui
                            .add_enabled(idx + 1 < binding_len, egui::Button::new("↓"))
                            .clicked()
                        {
                            reorder_request = Some((idx, idx + 1));
                        }
                        if ui.button("Edit").clicked() {
                            edit_request = Some((idx, binding.clone()));
                        }
                        if ui.button("Remove").clicked() {
                            remove_idx = Some(idx);
                        }
                    });
                    ui.separator();
                }
            });

        if binding_enabled_changed {
            *save_now = true;
        }

        if let Some((from, to)) = reorder_request {
            if from < entry.bindings.len() && to < entry.bindings.len() {
                entry.bindings.swap(from, to);
                *save_now = true;
            }
        }

        if let Some((idx, binding)) = edit_request {
            binding_dialog.open_edit(gesture_idx, &binding, idx);
        }
        if let Some(idx) = remove_idx {
            entry.bindings.remove(idx);
            *save_now = true;
        }
    }

    fn binding_dialog_ui(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        save_now: &mut bool,
    ) {
        if !self.binding_dialog.open {
            return;
        }

        let Some(gesture_idx) = self.binding_dialog.gesture_idx else {
            self.binding_dialog.open = false;
            return;
        };

        if gesture_idx >= self.db.gestures.len() {
            self.binding_dialog.open = false;
            return;
        }

        let gesture_label = self.db.gestures[gesture_idx].label.clone();
        let mut open = self.binding_dialog.open;
        let mut close_requested = false;
        egui::Window::new(format!("Bind Action: {gesture_label}"))
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let editor = &mut self.binding_dialog.editor;
                let mut save_entry: Option<BindingEntry> = None;
                ui.horizontal(|ui| {
                    ui.label("Label");
                    let response = ui.text_edit_singleline(&mut editor.label);
                    if editor.focus_label {
                        response.request_focus();
                        editor.focus_label = false;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Type");
                    ui.radio_value(&mut editor.kind, BindingKind::Execute, "Execute");
                    ui.radio_value(&mut editor.kind, BindingKind::SetQuery, "Set query");
                    ui.radio_value(
                        &mut editor.kind,
                        BindingKind::SetQueryAndShow,
                        "Set query + show",
                    );
                    ui.radio_value(
                        &mut editor.kind,
                        BindingKind::SetQueryAndExecute,
                        "Set query + execute",
                    );
                    ui.radio_value(
                        &mut editor.kind,
                        BindingKind::ToggleLauncher,
                        "Toggle launcher",
                    );
                });
                match editor.kind {
                    BindingKind::Execute => {
                        ui.horizontal(|ui| {
                            ui.label("Action");
                            ui.text_edit_singleline(&mut editor.action);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Args");
                            ui.text_edit_singleline(&mut editor.args);
                        });
                    }
                    BindingKind::SetQuery
                    | BindingKind::SetQueryAndShow
                    | BindingKind::SetQueryAndExecute => {
                        ui.horizontal(|ui| {
                            ui.label("Query");
                            ui.text_edit_singleline(&mut editor.action);
                        });
                    }
                    BindingKind::ToggleLauncher => {
                        ui.label("No action details required for toggling the launcher.");
                    }
                }
                ui.horizontal(|ui| {
                    ui.checkbox(&mut editor.enabled, "Enabled");
                });
                ui.separator();
                if editor.kind != BindingKind::ToggleLauncher {
                    let picker_label = match editor.kind {
                        BindingKind::Execute => "Pick an action",
                        BindingKind::SetQuery
                        | BindingKind::SetQueryAndShow
                        | BindingKind::SetQueryAndExecute => "Pick a query",
                        BindingKind::ToggleLauncher => "Pick an action",
                    };
                    ui.label(picker_label);
                    ui.horizontal(|ui| {
                        ui.label("Category");
                        let mut plugin_names: Vec<_> =
                            app.plugins.iter().map(|p| p.name().to_string()).collect();
                        plugin_names.push("app".to_string());
                        plugin_names.sort_unstable();
                        egui::ComboBox::from_id_source("mg_binding_category")
                            .selected_text(if editor.add_plugin.is_empty() {
                                "Select".to_string()
                            } else {
                                editor.add_plugin.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in plugin_names.iter() {
                                    ui.selectable_value(
                                        &mut editor.add_plugin,
                                        name.to_string(),
                                        name,
                                    );
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Filter");
                        ui.text_edit_singleline(&mut editor.add_filter);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Args");
                        ui.text_edit_singleline(&mut editor.add_args);
                    });
                    if editor.add_plugin == "app" {
                        let filter = editor.add_filter.trim().to_lowercase();
                        egui::ScrollArea::vertical()
                            .id_source("mg_binding_app_list")
                            .max_height(160.0)
                            .show(ui, |ui| {
                                for act in app.actions.iter() {
                                    if !filter.is_empty()
                                        && !act.label.to_lowercase().contains(&filter)
                                        && !act.desc.to_lowercase().contains(&filter)
                                        && !act.action.to_lowercase().contains(&filter)
                                    {
                                        continue;
                                    }
                                    if ui.button(format!("{} - {}", act.label, act.desc)).clicked()
                                    {
                                        editor.label = act.label.clone();
                                        if let Some(query) = act.action.strip_prefix("query:") {
                                            editor.kind = match editor.kind {
                                                BindingKind::SetQueryAndShow => {
                                                    BindingKind::SetQueryAndShow
                                                }
                                                BindingKind::SetQueryAndExecute => {
                                                    BindingKind::SetQueryAndExecute
                                                }
                                                _ => BindingKind::SetQuery,
                                            };
                                            editor.action = query.to_string();
                                            editor.args.clear();
                                        } else if act.action == "launcher:toggle" {
                                            editor.kind = BindingKind::ToggleLauncher;
                                            editor.action.clear();
                                            editor.args.clear();
                                        } else {
                                            editor.kind = BindingKind::Execute;
                                            editor.action = act.action.clone();
                                            editor.args = act.args.clone().unwrap_or_default();
                                        }
                                        editor.add_args.clear();
                                    }
                                }
                            });
                    } else if let Some(plugin) =
                        app.plugins.iter().find(|p| p.name() == editor.add_plugin)
                    {
                        let filter = editor.add_filter.trim().to_lowercase();
                        let mut actions = if plugin.name() == "folders" {
                            plugin.search(&format!("f list {}", editor.add_filter))
                        } else if plugin.name() == "bookmarks" {
                            plugin.search(&format!("bm list {}", editor.add_filter))
                        } else {
                            plugin.commands()
                        };
                        egui::ScrollArea::vertical()
                            .id_source("mg_binding_action_list")
                            .max_height(160.0)
                            .show(ui, |ui| {
                                for act in actions.drain(..) {
                                    if !filter.is_empty()
                                        && !act.label.to_lowercase().contains(&filter)
                                        && !act.desc.to_lowercase().contains(&filter)
                                        && !act.action.to_lowercase().contains(&filter)
                                    {
                                        continue;
                                    }
                                    if ui.button(format!("{} - {}", act.label, act.desc)).clicked()
                                    {
                                        let args = if editor.add_args.trim().is_empty() {
                                            None
                                        } else {
                                            Some(editor.add_args.clone())
                                        };

                                        if let Some(query) = act.action.strip_prefix("query:") {
                                            let mut query = query.to_string();
                                            if let Some(ref a) = args {
                                                query.push_str(a);
                                            }
                                            editor.kind = match editor.kind {
                                                BindingKind::SetQueryAndShow => {
                                                    BindingKind::SetQueryAndShow
                                                }
                                                BindingKind::SetQueryAndExecute => {
                                                    BindingKind::SetQueryAndExecute
                                                }
                                                _ => BindingKind::SetQuery,
                                            };
                                            editor.action = query;
                                            editor.args.clear();
                                        } else if act.action == "launcher:toggle" {
                                            editor.kind = BindingKind::ToggleLauncher;
                                            editor.action.clear();
                                            editor.args.clear();
                                        } else {
                                            editor.kind = BindingKind::Execute;
                                            editor.action = act.action.clone();
                                            editor.args = args.unwrap_or_else(|| {
                                                act.args.clone().unwrap_or_default()
                                            });
                                        }
                                        editor.label = act.label.clone();
                                        editor.add_args.clear();
                                    }
                                }
                            });
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
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
                            app.set_error("Label and action required".into());
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
                            let entry = BindingEntry {
                                label: editor.label.trim().to_string(),
                                kind: editor.kind,
                                action,
                                args,
                                enabled: editor.enabled,
                            };
                            save_entry = Some(entry);
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close_requested = true;
                    }
                });

                if let Some(binding_entry) = save_entry {
                    let bindings = &mut self.db.gestures[gesture_idx].bindings;
                    if let Some(edit_idx) = editor.edit_idx {
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

        if !open || close_requested {
            self.binding_dialog.editor.reset();
            self.binding_dialog.open = false;
        } else {
            self.binding_dialog.open = open;
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        let mut save_now = false;
        let mut open = self.open;
        egui::Window::new("Mouse Gestures")
            .default_size(egui::vec2(720.0, 420.0))
            .min_size(egui::vec2(520.0, 320.0))
            .max_size(egui::vec2(980.0, 760.0))
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Add Gesture").clicked() {
                        self.add_gesture();
                        save_now = true;
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
                ui.separator();

                // Use the remaining window height *before* entering `ui.horizontal`,
                // otherwise `ui.available_height()` inside the horizontal row will be only a single-row height.
                let content_height = ui.available_height();

                ui.horizontal(|ui| {
                    // Left panel: allocate an exact rect (full height) and render into a child Ui so it clips.
                    // This prevents long gesture labels/buttons from painting over the right panel.
                    let left_width = (ui.available_width() * 0.42).clamp(260.0, 380.0);
                    let (left_rect, _) = ui.allocate_exact_size(
                        egui::vec2(left_width, content_height),
                        egui::Sense::hover(),
                    );
                    let mut left_ui =
                        ui.child_ui(left_rect, egui::Layout::top_down(egui::Align::Min));
                    // Hard clip the left panel to its allocated rect so wide widgets
                    // (e.g. rename row text edit + buttons) don't paint over the right panel.
                    let left_clip = left_rect.shrink(1.0);
                    left_ui.set_clip_rect(left_clip);
                    left_ui.set_min_width(left_width);
                    left_ui.set_min_height(content_height);

                    left_ui.label("Gestures");
                    egui::ScrollArea::vertical()
                        .id_source("mg_gestures_list")
                        .auto_shrink([false, false])
                        .max_height(left_ui.available_height())
                        .show(&mut left_ui, |ui| {
                            // ScrollArea creates its own child Ui; re-apply the left clip
                            // so horizontally-wide rows can't paint into the right panel.
                            ui.set_clip_rect(left_clip);
                            let mut request_delete_idx: Option<usize> = None;
                            let gesture_order = self.sorted_gesture_indices();
                            for idx in gesture_order {
                                let selected = self.selected_idx == Some(idx);
                                let entry = &mut self.db.gestures[idx];
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut entry.enabled, "").changed() {
                                        save_now = true;
                                    }
                                    if ui
                                        .selectable_label(selected, format_gesture_label(entry))
                                        .clicked()
                                    {
                                        self.selected_idx = Some(idx);
                                        self.recorder.set_dir_mode(entry.dir_mode);
                                        self.token_buffer = entry.tokens.clone();
                                        self.binding_dialog.editor.reset();
                                        self.binding_dialog.open = false;
                                    }
                                    if ui.button("Rename").clicked() {
                                        self.rename_idx = Some(idx);
                                        self.rename_label = entry.label.clone();
                                    }
                                    if ui.button("Delete").clicked() {
                                        request_delete_idx = Some(idx);
                                    }
                                });
                                if self.rename_idx == Some(idx) {
                                    // Use a vertical group for renaming so the text edit never
                                    // pushes the Save/Cancel buttons past the left panel width.
                                    ui.group(|ui| {
                                        ui.label("Label");
                                        ui.add_sized(
                                            [ui.available_width(), 0.0],
                                            egui::TextEdit::singleline(&mut self.rename_label),
                                        );
                                        ui.horizontal(|ui| {
                                            if ui.button("Save").clicked() {
                                                if !self.rename_label.trim().is_empty() {
                                                    entry.label =
                                                        self.rename_label.trim().to_string();
                                                    self.rename_idx = None;
                                                    save_now = true;
                                                }
                                            }
                                            if ui.button("Cancel").clicked() {
                                                self.rename_idx = None;
                                            }
                                        });
                                    });
                                }
                            }
                            if let Some(idx) = request_delete_idx {
                                let _ = self.queue_gesture_delete(idx);
                            }
                        });
                    ui.separator();

                    // Right panel: allocate an exact rect (full height) and render into a child Ui
                    // so it clips too. This prevents any over-wide widgets from bleeding left/right.
                    let (right_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), content_height),
                        egui::Sense::hover(),
                    );
                    let mut right_ui =
                        ui.child_ui(right_rect, egui::Layout::top_down(egui::Align::Min));
                    right_ui.set_min_height(content_height);
                    right_ui.set_min_width(320.0);
                    right_ui.set_clip_rect(right_rect.shrink(1.0));

                    egui::ScrollArea::both()
                        .id_source("mg_right_panel")
                        .auto_shrink([false, false])
                        .show(&mut right_ui, |ui| {
                            if let Some(idx) = self.selected_idx {
                                if let Some(entry) = self.db.gestures.get_mut(idx) {
                                    ui.label("Recorder");
                                    ui.horizontal(|ui| {
                                        ui.label("Direction mode");
                                        let mut dir_mode = entry.dir_mode;
                                        egui::ComboBox::from_id_source("mg_dir_mode")
                                            .selected_text(match dir_mode {
                                                DirMode::Four => "4-way",
                                                DirMode::Eight => "8-way",
                                            })
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(
                                                    &mut dir_mode,
                                                    DirMode::Four,
                                                    "4-way",
                                                );
                                                ui.selectable_value(
                                                    &mut dir_mode,
                                                    DirMode::Eight,
                                                    "8-way",
                                                );
                                            });
                                        if dir_mode != entry.dir_mode {
                                            entry.dir_mode = dir_mode;
                                            self.recorder.set_dir_mode(dir_mode);
                                            save_now = true;
                                        }
                                    });
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
                                    ui.horizontal(|ui| {
                                        if ui.button("Use Recording").clicked() {
                                            entry.tokens = recorded_tokens.clone();
                                            self.token_buffer = entry.tokens.clone();
                                            entry.stroke = self.recorder.normalized_stroke();
                                            self.recorder.reset();
                                            save_now = true;
                                        }
                                        if ui.button("Clear Recording").clicked() {
                                            self.recorder.reset();
                                        }
                                        if !entry.stroke.is_empty()
                                            && ui.button("Clear Saved").clicked()
                                        {
                                            entry.stroke.clear();
                                            save_now = true;
                                        }
                                    });
                                    let available = ui.available_width();
                                    let size = egui::vec2(available.max(320.0), 260.0);
                                    let (rect, response) =
                                        ui.allocate_exact_size(size, egui::Sense::drag());
                                    let painter = ui.painter_at(rect);
                                    painter.rect_stroke(
                                        rect,
                                        0.0,
                                        egui::Stroke::new(1.0, egui::Color32::GRAY),
                                    );
                                    if response.drag_started() {
                                        // Starting a new recording replaces any existing saved preview stroke.
                                        if !entry.stroke.is_empty() {
                                            entry.stroke.clear();
                                            save_now = true;
                                        }
                                        self.recorder.reset();
                                        if let Some(pos) = response.interact_pointer_pos() {
                                            self.recorder.push_point(pos);
                                        }
                                    }
                                    if response.dragged() {
                                        if let Some(pos) = response.interact_pointer_pos() {
                                            self.recorder.push_point(pos);
                                        }
                                    }
                                    if response.drag_stopped() {
                                        let recorded_tokens = self.recorder.tokens_string();
                                        if !recorded_tokens.is_empty() {
                                            entry.tokens = recorded_tokens.clone();
                                            self.token_buffer = entry.tokens.clone();
                                            entry.stroke = self.recorder.normalized_stroke();
                                            save_now = true;
                                        }
                                        //Clear the live drawing so the saved preview is shown
                                        //immediately
                                        self.recorder.reset();
                                    }

                                    // Render the saved preview stroke (if any) behind the active recording.
                                    if entry.stroke.len() >= 2 {
                                        let pts = stroke_points_in_rect(&entry.stroke, rect);
                                        if pts.len() >= 2 {
                                            painter.add(egui::Shape::line(
                                                pts,
                                                egui::Stroke::new(
                                                    2.0,
                                                    egui::Color32::from_gray(140),
                                                ),
                                            ));
                                        }
                                    }
                                    if self.recorder.points().len() >= 2 {
                                        painter.add(egui::Shape::line(
                                            self.recorder.points().to_vec(),
                                            egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE),
                                        ));
                                    }
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.label("Tokens");
                                        ui.text_edit_singleline(&mut self.token_buffer);
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.button("Import").clicked() {
                                            entry.tokens = self.token_buffer.trim().to_string();
                                            save_now = true;
                                        }
                                        if ui.button("Export").clicked() {
                                            self.token_buffer = entry.tokens.clone();
                                            ctx.output_mut(|o| {
                                                o.copied_text = self.token_buffer.clone();
                                            });
                                        }
                                    });
                                    ui.separator();
                                    let binding_dialog = &mut self.binding_dialog;
                                    Self::bindings_ui(
                                        binding_dialog,
                                        ui,
                                        entry,
                                        idx,
                                        &mut save_now,
                                    );
                                } else {
                                    ui.label("Select a gesture to edit.");
                                }
                            } else {
                                ui.label("Select a gesture to edit.");
                            }
                        });
                });
            });
        self.binding_dialog_ui(ctx, app, &mut save_now);
        match self.delete_confirm_modal.ui(ctx) {
            ConfirmationResult::Confirmed => {
                if self.apply_pending_gesture_delete() {
                    save_now = true;
                }
            }
            ConfirmationResult::Cancelled => self.cancel_pending_gesture_delete(),
            ConfirmationResult::None => {}
        }
        if save_now {
            self.save(app);
        }
        if close {
            self.open = false;
        } else {
            self.open = open;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert!(dlg.queue_gesture_delete(0));
        assert_eq!(dlg.db.gestures.len(), 1);
        assert!(dlg.pending_delete.is_some());
    }

    #[test]
    fn cancelling_gesture_delete_keeps_db_and_selection() {
        let mut dlg = MgGesturesDialog::default();
        dlg.db.gestures = vec![gesture("A", "R"), gesture("B", "L")];
        dlg.selected_idx = Some(1);
        dlg.rename_idx = Some(1);

        assert!(dlg.queue_gesture_delete(1));
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

        assert!(dlg.queue_gesture_delete(1));
        assert!(dlg.apply_pending_gesture_delete());

        assert_eq!(dlg.db.gestures.len(), 2);
        assert_eq!(dlg.selected_idx, Some(1));
        assert_eq!(dlg.rename_idx, Some(1));
    }
}
