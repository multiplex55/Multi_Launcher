use crate::gui::LauncherApp;
use crate::mouse_gestures::db::{
    format_gesture_label, load_gestures, save_gestures, GestureDb, GestureEntry, GESTURES_FILE,
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

pub struct MgGesturesDialog {
    pub open: bool,
    db: GestureDb,
    selected_idx: Option<usize>,
    rename_idx: Option<usize>,
    rename_label: String,
    recorder: GestureRecorder,
    token_buffer: String,
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
        self.ensure_selection();
    }

    pub fn open_add(&mut self) {
        self.open();
        self.add_gesture();
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
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_gestures(GESTURES_FILE, &self.db) {
            app.set_error(format!("Failed to save mouse gestures: {e}"));
        } else {
            app.search();
            app.focus_input();
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
                                    let mut remove_idx: Option<usize> = None;
                                    let gesture_order = self.sorted_gesture_indices();
                                    for idx in gesture_order {
                                        let selected = self.selected_idx == Some(idx);
                                        let entry = &mut self.db.gestures[idx];
                                        ui.horizontal(|ui| {
                                            if ui.checkbox(&mut entry.enabled, "").changed() {
                                                save_now = true;
                                            }
                                            if ui
                                                .selectable_label(
                                                    selected,
                                                    format_gesture_label(entry),
                                                )
                                                .clicked()
                                            {
                                                self.selected_idx = Some(idx);
                                                self.recorder.set_dir_mode(entry.dir_mode);
                                                self.token_buffer = entry.tokens.clone();
                                            }
                                            if ui.button("Rename").clicked() {
                                                self.rename_idx = Some(idx);
                                                self.rename_label = entry.label.clone();
                                            }
                                            if ui.button("Delete").clicked() {
                                                remove_idx = Some(idx);
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
                                    if let Some(idx) = remove_idx {
                                        self.db.gestures.remove(idx);

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
                                        save_now = true;
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

                    let ui = &mut right_ui;
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
                                            egui::Stroke::new(2.0, egui::Color32::from_gray(140)),
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
                            } else {
                                ui.label("Select a gesture to edit.");
                            }
                        } else {
                            ui.label("Select a gesture to edit.");
                        }
                });
            });
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
