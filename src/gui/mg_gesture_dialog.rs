use crate::mouse_gestures::mouse_gesture_service;
use crate::plugins::mouse_gestures::db::{
    load_gestures, save_gestures, MouseGestureDb, MOUSE_GESTURES_FILE,
};
use crate::plugins::mouse_gestures::engine::{
    parse_gesture, serialize_gesture, GestureDefinition, ParseError, Point,
};
use eframe::egui;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct MouseGesturesGestureDialog {
    pub open: bool,
    loaded: bool,
    db: MouseGestureDb,
    selected_gesture: Option<String>,
    gesture_name: String,
    gesture_text: String,
    points: Vec<Point>,
    status: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_serialized_updates_state() {
        let mut name = String::new();
        let mut points = Vec::new();
        let serialized = "zig:0,0|10,0|10,10";
        apply_serialized_gesture(&mut name, &mut points, serialized).unwrap();
        assert_eq!(name, "zig");
        assert_eq!(points.len(), 3);
    }

    #[test]
    fn apply_serialized_reports_invalid_input() {
        let mut name = String::new();
        let mut points = Vec::new();
        let err = apply_serialized_gesture(&mut name, &mut points, "").unwrap_err();
        assert!(matches!(
            err.kind,
            crate::plugins::mouse_gestures::engine::ParseErrorKind::EmptyInput
        ));
    }
}

impl MouseGesturesGestureDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.loaded = false;
    }

    fn load_db(&mut self) {
        self.db = load_gestures(MOUSE_GESTURES_FILE).unwrap_or_default();
        self.loaded = true;
    }

    fn persist_db(&mut self, app: &mut crate::gui::LauncherApp) {
        sync_gesture_ids(&mut self.db);
        if let Err(e) = save_gestures(MOUSE_GESTURES_FILE, &self.db) {
            app.set_error(format!("Failed to save gestures: {e}"));
        } else {
            mouse_gesture_service().update_db(self.db.clone());
        }
    }

    fn load_selected(&mut self, gesture_id: &str) {
        if let Some(serialized) = self.db.bindings.get(gesture_id) {
            match apply_serialized_gesture(&mut self.gesture_name, &mut self.points, serialized) {
                Ok(()) => {
                    self.gesture_text = serialized.clone();
                }
                Err(err) => {
                    self.status = Some(format!("Failed to parse gesture: {err}"));
                }
            }
        }
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut crate::gui::LauncherApp) {
        if !self.open {
            return;
        }
        if !self.loaded {
            self.load_db();
        }

        let mut open = self.open;
        egui::Window::new("Mouse Gesture Recorder")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Gesture name");
                    ui.text_edit_singleline(&mut self.gesture_name);
                });
                ui.label("Draw gesture");

                let canvas_size = egui::vec2(360.0, 200.0);
                let (rect, response) = ui.allocate_exact_size(canvas_size, egui::Sense::drag());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

                if response.drag_started() {
                    self.points.clear();
                }
                if response.dragged() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let point = Point {
                            x: pos.x - rect.left(),
                            y: pos.y - rect.top(),
                        };
                        if self
                            .points
                            .last()
                            .map(|p| (p.x - point.x).abs() + (p.y - point.y).abs() > 1.0)
                            .unwrap_or(true)
                        {
                            self.points.push(point);
                        }
                    }
                }
                if response.dragged_stopped() {
                    self.update_serialized();
                }

                draw_points(&painter, &self.points, rect);

                ui.horizontal(|ui| {
                    if ui.button("Clear").clicked() {
                        self.points.clear();
                        self.gesture_text.clear();
                        self.set_status("Cleared current gesture.");
                    }
                    if ui.button("Use recorded").clicked() {
                        self.update_serialized();
                    }
                });

                ui.separator();
                ui.label("Serialized gesture");
                ui.text_edit_multiline(&mut self.gesture_text);
                if ui.button("Import from text").clicked() {
                    match apply_serialized_gesture(
                        &mut self.gesture_name,
                        &mut self.points,
                        &self.gesture_text,
                    ) {
                        Ok(()) => self.set_status("Imported gesture from text."),
                        Err(err) => self.set_status(format!("Import failed: {err}")),
                    }
                }

                ui.separator();
                ui.label("Gesture library");
                let labels = gesture_labels(&self.db);
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for (gesture_id, label) in labels {
                            let selected = self
                                .selected_gesture
                                .as_deref()
                                .map(|id| id == gesture_id)
                                .unwrap_or(false);
                            if ui
                                .selectable_label(selected, format!("{label} ({gesture_id})"))
                                .clicked()
                            {
                                self.selected_gesture = Some(gesture_id.clone());
                                self.load_selected(&gesture_id);
                            }
                        }
                    });

                ui.horizontal(|ui| {
                    if ui.button("New").clicked() {
                        self.selected_gesture = None;
                        self.gesture_name.clear();
                        self.gesture_text.clear();
                        self.points.clear();
                    }
                    if ui.button("Save").clicked() {
                        if self.gesture_text.trim().is_empty() {
                            self.update_serialized();
                        }
                        if self.gesture_text.trim().is_empty() {
                            self.set_status("Cannot save an empty gesture.");
                        } else {
                            let gesture_id = self
                                .selected_gesture
                                .clone()
                                .unwrap_or_else(|| next_gesture_id(&self.db));
                            self.db
                                .bindings
                                .insert(gesture_id.clone(), self.gesture_text.trim().to_string());
                            self.selected_gesture = Some(gesture_id);
                            self.persist_db(app);
                            self.set_status("Gesture saved.");
                        }
                    }
                    if ui.button("Delete").clicked() {
                        if let Some(id) = self.selected_gesture.take() {
                            self.db.bindings.remove(&id);
                            self.persist_db(app);
                            self.gesture_text.clear();
                            self.points.clear();
                            self.set_status("Gesture deleted.");
                        }
                    }
                });

                if let Some(status) = &self.status {
                    ui.label(status);
                }
            });
        self.open = open;
    }

    fn update_serialized(&mut self) {
        if self.points.is_empty() {
            self.gesture_text.clear();
            return;
        }
        let name = self.gesture_name.trim();
        let gesture = GestureDefinition {
            name: if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            },
            points: self.points.clone(),
        };
        self.gesture_text = serialize_gesture(&gesture);
    }
}

fn apply_serialized_gesture(
    gesture_name: &mut String,
    points: &mut Vec<Point>,
    serialized: &str,
) -> Result<(), ParseError> {
    let parsed = parse_gesture(serialized)?;
    *gesture_name = parsed.name.unwrap_or_default();
    *points = parsed.points;
    Ok(())
}

fn gesture_labels(db: &MouseGestureDb) -> Vec<(String, String)> {
    let mut labels = BTreeMap::new();
    for (id, serialized) in &db.bindings {
        let label = parse_gesture(serialized)
            .ok()
            .and_then(|g| g.name)
            .unwrap_or_else(|| "(unnamed)".to_string());
        labels.insert(id.clone(), label);
    }
    labels.into_iter().collect()
}

fn next_gesture_id(db: &MouseGestureDb) -> String {
    let mut index = db.bindings.len() + 1;
    loop {
        let candidate = format!("gesture_{index}");
        if !db.bindings.contains_key(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn sync_gesture_ids(db: &mut MouseGestureDb) {
    let mut ids: Vec<String> = db.bindings.keys().cloned().collect();
    ids.sort();
    db.gestures = ids;
}

fn draw_points(painter: &egui::Painter, points: &[Point], rect: egui::Rect) {
    if points.len() < 2 {
        return;
    }
    let stroke = egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE);
    for pair in points.windows(2) {
        let a = egui::pos2(rect.left() + pair[0].x, rect.top() + pair[0].y);
        let b = egui::pos2(rect.left() + pair[1].x, rect.top() + pair[1].y);
        painter.line_segment([a, b], stroke);
    }
}
