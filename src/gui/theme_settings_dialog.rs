use crate::settings::{ColorScheme, Settings, ThemeColor, ThemeMode, ThemeSettings};
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]

fn theme_json_path(settings_path: &str) -> PathBuf {
    let path = Path::new(settings_path);
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join("theme_settings.json")
}

fn persist_theme_json(settings_path: &str, theme: &ThemeSettings) -> Result<(), String> {
    let theme_path = theme_json_path(settings_path);
    let json = serde_json::to_string_pretty(theme)
        .map_err(|err| format!("Failed to serialize theme settings: {err}"))?;
    std::fs::write(&theme_path, json).map_err(|err| {
        format!(
            "Failed to save theme file ({}): {err}",
            theme_path.display()
        )
    })
}

pub struct ThemeSettingsDialogState {
    pub draft: ThemeSettings,
    pub dirty: bool,
    pub last_error: Option<String>,
    needs_reload: bool,
}

impl Default for ThemeSettingsDialogState {
    fn default() -> Self {
        Self {
            draft: ThemeSettings::default(),
            dirty: false,
            last_error: None,
            needs_reload: true,
        }
    }
}

impl ThemeSettingsDialogState {
    pub fn request_reload(&mut self) {
        self.needs_reload = true;
    }

    pub fn reload_from_path(&mut self, settings_path: &str) {
        self.last_error = None;
        match Settings::load(settings_path) {
            Ok(settings) => {
                self.draft = settings.theme;
                self.dirty = false;
                self.needs_reload = false;
            }
            Err(err) => {
                self.last_error = Some(format!("Failed to load settings: {err}"));
                self.needs_reload = false;
            }
        }
    }

    pub fn save_to_path(&mut self, settings_path: &str) -> Result<(), String> {
        self.last_error = None;
        let mut settings = Settings::load(settings_path)
            .map_err(|err| format!("Failed to load settings: {err}"))?;
        settings.theme = self.draft.clone();
        settings
            .save(settings_path)
            .map_err(|err| format!("Failed to save settings: {err}"))?;
        persist_theme_json(settings_path, &self.draft)?;
        self.dirty = false;
        Ok(())
    }

    fn active_scheme_mut(&mut self) -> &mut ColorScheme {
        match self.draft.mode {
            ThemeMode::Custom => &mut self.draft.custom_scheme,
            ThemeMode::Dark | ThemeMode::System => self
                .draft
                .named_presets
                .entry("dark".to_string())
                .or_insert_with(ColorScheme::dark),
            ThemeMode::Light => self
                .draft
                .named_presets
                .entry("light".to_string())
                .or_insert_with(ColorScheme::light),
        }
    }

    fn active_scheme(&self) -> ColorScheme {
        match self.draft.mode {
            ThemeMode::Custom => self.draft.custom_scheme.clone(),
            ThemeMode::Dark | ThemeMode::System => self
                .draft
                .named_presets
                .get("dark")
                .cloned()
                .unwrap_or_else(ColorScheme::dark),
            ThemeMode::Light => self
                .draft
                .named_presets
                .get("light")
                .cloned()
                .unwrap_or_else(ColorScheme::light),
        }
    }
}

pub fn ui(
    ctx: &egui::Context,
    app: &mut crate::gui::LauncherApp,
    open: &mut bool,
    state: &mut ThemeSettingsDialogState,
) {
    if !*open {
        return;
    }
    if state.needs_reload {
        state.reload_from_path(&app.settings_path);
    }

    let mut keep_open = *open;
    egui::Window::new("Theme settings")
        .open(&mut keep_open)
        .resizable(true)
        .default_width(640.0)
        .show(ctx, |ui| {
            if let Some(err) = &state.last_error {
                ui.colored_label(egui::Color32::RED, err);
                ui.separator();
            }

            ui.label("Customize launcher colors and theme mode.");
            ui.separator();

            let mut changed = false;
            changed |= section_base_mode(ui, &mut state.draft.mode);
            ui.separator();
            let mode = state.draft.mode;
            let scheme = state.active_scheme_mut();
            changed |= section_core_surfaces(ui, scheme, mode);
            ui.separator();
            changed |= section_text_and_links(ui, scheme, mode);
            ui.separator();
            changed |= section_widgets(ui, scheme, mode);
            ui.separator();
            changed |= section_selection(ui, scheme, mode);
            ui.separator();
            changed |= section_semantic(ui, scheme, mode);
            ui.separator();
            preview(ui, &state.active_scheme());

            if changed {
                state.dirty = true;
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Reset all to default").clicked() {
                    state.draft = ThemeSettings::default();
                    state.dirty = true;
                }
                if ui.button("Cancel").clicked() {
                    *open = false;
                    state.request_reload();
                }
                if ui
                    .add_enabled(state.dirty, egui::Button::new("Apply"))
                    .clicked()
                {
                    match state.save_to_path(&app.settings_path) {
                        Ok(_) => {
                            app.apply_theme_visuals(ctx, &state.draft);
                            if app.enable_toasts {
                                app.add_toast(Toast {
                                    text: "Theme settings applied".into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(app.toast_duration as f64),
                                });
                            }
                        }
                        Err(err) => {
                            state.last_error = Some(err.clone());
                            if app.enable_toasts {
                                app.add_toast(Toast {
                                    text: err.into(),
                                    kind: ToastKind::Error,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(app.toast_duration as f64),
                                });
                            }
                        }
                    }
                }
                if ui
                    .add_enabled(state.dirty, egui::Button::new("Save"))
                    .clicked()
                {
                    match state.save_to_path(&app.settings_path) {
                        Ok(_) => {
                            app.apply_theme_visuals(ctx, &state.draft);
                            if app.enable_toasts {
                                app.add_toast(Toast {
                                    text: "Theme settings saved".into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(app.toast_duration as f64),
                                });
                            }
                            *open = false;
                        }
                        Err(err) => {
                            state.last_error = Some(err.clone());
                            if app.enable_toasts {
                                app.add_toast(Toast {
                                    text: err.into(),
                                    kind: ToastKind::Error,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(app.toast_duration as f64),
                                });
                            }
                        }
                    }
                }
            });
        });

    *open = keep_open && *open;
}

fn defaults_for_mode(mode: ThemeMode) -> ColorScheme {
    match mode {
        ThemeMode::Light => ColorScheme::light(),
        ThemeMode::Dark | ThemeMode::Custom | ThemeMode::System => ColorScheme::dark(),
    }
}

fn section_base_mode(ui: &mut egui::Ui, mode: &mut ThemeMode) -> bool {
    let mut changed = false;
    ui.heading("Base mode");
    ui.horizontal(|ui| {
        changed |= ui
            .selectable_value(mode, ThemeMode::System, "System")
            .changed();
        changed |= ui.selectable_value(mode, ThemeMode::Dark, "Dark").changed();
        changed |= ui
            .selectable_value(mode, ThemeMode::Light, "Light")
            .changed();
        changed |= ui
            .selectable_value(mode, ThemeMode::Custom, "Custom")
            .changed();
    });
    changed
}

fn section_core_surfaces(ui: &mut egui::Ui, scheme: &mut ColorScheme, mode: ThemeMode) -> bool {
    let mut changed = false;
    ui.heading("Core surfaces");
    changed |= color_row(ui, "Window background", &mut scheme.window_fill);
    changed |= color_row(ui, "Panel background", &mut scheme.panel_fill);
    if ui.small_button("Reset section defaults").clicked() {
        let defaults = defaults_for_mode(mode);
        scheme.window_fill = defaults.window_fill;
        scheme.panel_fill = defaults.panel_fill;
        changed = true;
    }
    changed
}

fn section_text_and_links(ui: &mut egui::Ui, scheme: &mut ColorScheme, mode: ThemeMode) -> bool {
    let mut changed = false;
    ui.heading("Text and links");
    changed |= color_row(ui, "Text", &mut scheme.text);
    changed |= color_row(ui, "Hyperlink", &mut scheme.hyperlink);
    if ui.small_button("Reset section defaults").clicked() {
        let defaults = defaults_for_mode(mode);
        scheme.text = defaults.text;
        scheme.hyperlink = defaults.hyperlink;
        changed = true;
    }
    changed
}

fn section_widgets(ui: &mut egui::Ui, scheme: &mut ColorScheme, mode: ThemeMode) -> bool {
    let mut changed = false;
    ui.heading("Interactive widgets");
    changed |= color_row(ui, "Inactive fill", &mut scheme.widget_inactive_fill);
    changed |= color_row(ui, "Inactive stroke", &mut scheme.widget_inactive_stroke);
    changed |= color_row(ui, "Hovered fill", &mut scheme.widget_hovered_fill);
    changed |= color_row(ui, "Hovered stroke", &mut scheme.widget_hovered_stroke);
    changed |= color_row(ui, "Active fill", &mut scheme.widget_active_fill);
    changed |= color_row(ui, "Active stroke", &mut scheme.widget_active_stroke);
    if ui.small_button("Reset section defaults").clicked() {
        let defaults = defaults_for_mode(mode);
        scheme.widget_inactive_fill = defaults.widget_inactive_fill;
        scheme.widget_inactive_stroke = defaults.widget_inactive_stroke;
        scheme.widget_hovered_fill = defaults.widget_hovered_fill;
        scheme.widget_hovered_stroke = defaults.widget_hovered_stroke;
        scheme.widget_active_fill = defaults.widget_active_fill;
        scheme.widget_active_stroke = defaults.widget_active_stroke;
        changed = true;
    }
    changed
}

fn section_selection(ui: &mut egui::Ui, scheme: &mut ColorScheme, mode: ThemeMode) -> bool {
    let mut changed = false;
    ui.heading("Selection and highlight");
    changed |= color_row(ui, "Selection background", &mut scheme.selection_bg);
    changed |= color_row(ui, "Selection stroke", &mut scheme.selection_stroke);
    if ui.small_button("Reset section defaults").clicked() {
        let defaults = defaults_for_mode(mode);
        scheme.selection_bg = defaults.selection_bg;
        scheme.selection_stroke = defaults.selection_stroke;
        changed = true;
    }
    changed
}

fn section_semantic(ui: &mut egui::Ui, scheme: &mut ColorScheme, mode: ThemeMode) -> bool {
    let mut changed = false;
    ui.heading("Semantic accents");
    changed |= color_row(ui, "Success", &mut scheme.success_accent);
    changed |= color_row(ui, "Warning", &mut scheme.warn_accent);
    changed |= color_row(ui, "Error", &mut scheme.error_accent);
    if ui.small_button("Reset section defaults").clicked() {
        let defaults = defaults_for_mode(mode);
        scheme.success_accent = defaults.success_accent;
        scheme.warn_accent = defaults.warn_accent;
        scheme.error_accent = defaults.error_accent;
        changed = true;
    }
    changed
}

fn color_row(ui: &mut egui::Ui, label: &str, color: &mut ThemeColor) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let mut c = egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);
        if ui.color_edit_button_srgba(&mut c).changed() {
            color.r = c.r();
            color.g = c.g();
            color.b = c.b();
            color.a = c.a();
            changed = true;
        }
    });
    changed
}

fn preview(ui: &mut egui::Ui, scheme: &ColorScheme) {
    ui.heading("Live preview");
    egui::Frame::none()
        .fill(to_egui(scheme.window_fill))
        .stroke(egui::Stroke::new(
            1.0,
            to_egui(scheme.widget_inactive_stroke),
        ))
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Window text").color(to_egui(scheme.text)));
            ui.hyperlink_to(
                egui::RichText::new("Link sample").color(to_egui(scheme.hyperlink)),
                "https://example.com",
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                swatch(
                    ui,
                    "Inactive",
                    scheme.widget_inactive_fill,
                    scheme.widget_inactive_stroke,
                );
                swatch(
                    ui,
                    "Hovered",
                    scheme.widget_hovered_fill,
                    scheme.widget_hovered_stroke,
                );
                swatch(
                    ui,
                    "Active",
                    scheme.widget_active_fill,
                    scheme.widget_active_stroke,
                );
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                swatch(
                    ui,
                    "Selection",
                    scheme.selection_bg,
                    scheme.selection_stroke,
                );
                swatch(ui, "Success", scheme.success_accent, scheme.success_accent);
                swatch(ui, "Warning", scheme.warn_accent, scheme.warn_accent);
                swatch(ui, "Error", scheme.error_accent, scheme.error_accent);
            });
        });
}

fn swatch(ui: &mut egui::Ui, label: &str, fill: ThemeColor, stroke: ThemeColor) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(90.0, 26.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        4.0,
        to_egui(fill),
        egui::Stroke::new(1.0, to_egui(stroke)),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::TextStyle::Small.resolve(ui.style()),
        to_egui(ThemeColor {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
    );
}

fn to_egui(c: ThemeColor) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}
