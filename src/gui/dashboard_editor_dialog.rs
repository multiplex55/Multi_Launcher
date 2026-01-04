use crate::dashboard::config::{DashboardConfig, OverflowMode, SlotConfig};
use crate::dashboard::widgets::{WidgetRegistry, WidgetSettingsContext};
use eframe::egui;
use serde_json::Value;

pub struct DashboardEditorDialog {
    pub open: bool,
    path: String,
    config: DashboardConfig,
    error: Option<String>,
    pending_save: bool,
}

impl Default for DashboardEditorDialog {
    fn default() -> Self {
        Self {
            open: false,
            path: "dashboard.json".into(),
            config: DashboardConfig::default(),
            error: None,
            pending_save: false,
        }
    }
}

impl DashboardEditorDialog {
    pub fn open(&mut self, path: &str, registry: &WidgetRegistry) {
        self.path = path.to_string();
        self.reload(registry);
        self.open = true;
    }

    fn reload(&mut self, registry: &WidgetRegistry) {
        match DashboardConfig::load(&self.path, registry) {
            Ok(cfg) => {
                self.config = cfg;
                self.error = None;
            }
            Err(e) => {
                self.error = Some(format!("Failed to load dashboard: {e}"));
            }
        }
    }

    fn save(&mut self) {
        let tmp = format!("{}.tmp", self.path);
        if let Err(e) = self.config.save(&tmp) {
            self.error = Some(format!("Failed to save: {e}"));
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, &self.path) {
            self.error = Some(format!("Failed to finalize save: {e}"));
            return;
        }
        self.pending_save = true;
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        registry: &WidgetRegistry,
        settings_ctx: WidgetSettingsContext<'_>,
    ) -> bool {
        if !self.open {
            return false;
        }
        let mut should_reload = false;
        let mut open = self.open;
        egui::Window::new("Dashboard Editor")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(err) = &self.error {
                    ui.colored_label(egui::Color32::RED, err);
                }

                ui.horizontal(|ui| {
                    ui.label("Rows");
                    ui.add(egui::DragValue::new(&mut self.config.grid.rows).clamp_range(1..=12));
                    ui.label("Cols");
                    ui.add(egui::DragValue::new(&mut self.config.grid.cols).clamp_range(1..=12));
                });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Add slot").clicked() {
                        self.config
                            .slots
                            .push(SlotConfig::with_widget("weather_site", 0, 0));
                    }
                    if ui.button("Reload from disk").clicked() {
                        self.reload(registry);
                    }
                    if ui.button("Save").clicked() {
                        self.save();
                    }
                });

                ui.separator();
                let mut idx = 0;
                while idx < self.config.slots.len() {
                    let slot = &mut self.config.slots[idx];
                    let mut removed = false;
                    ui.push_id(idx, |ui| {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!("Slot {idx}"));
                                if ui.button("Remove").clicked() {
                                    removed = true;
                                }
                            });
                            egui::ComboBox::from_label("Widget type")
                                .selected_text(slot.widget.clone())
                                .show_ui(ui, |ui| {
                                    for name in registry.names() {
                                        ui.selectable_value(&mut slot.widget, name.clone(), name);
                                    }
                                });
                            ui.horizontal(|ui| {
                                ui.label("Row");
                                ui.add(egui::DragValue::new(&mut slot.row));
                                ui.label("Col");
                                ui.add(egui::DragValue::new(&mut slot.col));
                            });
                            ui.horizontal(|ui| {
                                ui.label("Row span");
                                ui.add(
                                    egui::DragValue::new(&mut slot.row_span).clamp_range(1..=12),
                                );
                                ui.label("Col span");
                                ui.add(
                                    egui::DragValue::new(&mut slot.col_span).clamp_range(1..=12),
                                );
                            });
                            egui::ComboBox::from_label("Overflow")
                                .selected_text(slot.overflow.as_str())
                                .show_ui(ui, |ui| {
                                    for mode in [
                                        OverflowMode::Scroll,
                                        OverflowMode::Clip,
                                        OverflowMode::Auto,
                                    ] {
                                        ui.selectable_value(
                                            &mut slot.overflow,
                                            mode,
                                            mode.as_str(),
                                        );
                                    }
                                });
                            ui.horizontal(|ui| {
                                let id = slot.id.get_or_insert_with(|| format!("slot-{idx}"));
                                ui.label("Label");
                                ui.text_edit_singleline(id);
                            });
                            ui.separator();
                            egui::CollapsingHeader::new("Settings")
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.button("Reset to defaults").clicked() {
                                            slot.settings = registry
                                                .default_settings(&slot.widget)
                                                .unwrap_or_else(|| {
                                                    Value::Object(Default::default())
                                                });
                                        }
                                        if slot.settings.is_null() {
                                            ui.colored_label(
                                                egui::Color32::YELLOW,
                                                "Settings were empty; defaults will be used.",
                                            );
                                            slot.settings = registry
                                                .default_settings(&slot.widget)
                                                .unwrap_or_else(|| {
                                                    Value::Object(Default::default())
                                                });
                                        }
                                    });

                                    if let Some(result) = registry.render_settings_ui(
                                        &slot.widget,
                                        ui,
                                        &mut slot.settings,
                                        &settings_ctx,
                                    ) {
                                        if let Some(err) = result.error {
                                            ui.colored_label(
                                                egui::Color32::YELLOW,
                                                format!("{err}. Settings saved after edits."),
                                            );
                                        }
                                    } else {
                                        ui.label("No settings available for this widget.");
                                    }
                                });
                        });
                    });
                    if removed {
                        self.config.slots.remove(idx);
                    } else {
                        idx += 1;
                    }
                }

                ui.separator();
                ui.label("Preview");
                let (_, mut warnings) =
                    crate::dashboard::layout::normalize_slots(&self.config, registry);
                if !warnings.is_empty() {
                    warnings.dedup();
                    for warn in warnings {
                        ui.colored_label(egui::Color32::YELLOW, warn);
                    }
                }
            });
        self.open = open;
        if self.pending_save {
            self.pending_save = false;
            should_reload = true;
        }
        should_reload
    }
}
