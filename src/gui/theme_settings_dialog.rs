use crate::gui::theme::widgets::{self, ButtonEmphasis, StatusTone};
use crate::gui::theme::{
    ChromeIcon, ResolvedTheme, SystemAppearance, TextRole, chrome_icon_button,
    chrome_small_icon_button, effective_color_scheme, resolve_theme, text_style_for_role,
};
use crate::settings::{ColorScheme, Settings, ThemeColor, ThemeMode, ThemeSettings};
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};

#[derive(Debug, Clone)]
pub struct ThemeSettingsDialogState {
    pub draft: ThemeSettings,
    pub dirty: bool,
    pub last_error: Option<String>,
    persisted_baseline: Option<ThemeSettings>,
    conflicted: bool,
}

impl Default for ThemeSettingsDialogState {
    fn default() -> Self {
        Self {
            draft: ThemeSettings::default(),
            dirty: false,
            last_error: None,
            persisted_baseline: None,
            conflicted: false,
        }
    }
}

impl ThemeSettingsDialogState {
    pub fn begin_session(&mut self, ctx: &egui::Context, settings_path: &str) {
        self.persisted_baseline = None;
        self.conflicted = false;
        self.dirty = false;
        self.last_error = None;
        self.reload_from_path(ctx, settings_path);
    }

    pub fn reload_from_path(&mut self, ctx: &egui::Context, settings_path: &str) {
        self.last_error = None;
        match Settings::load(settings_path) {
            Ok(settings) => {
                crate::gui::theme::publish_committed(ctx, &settings.theme);
                crate::gui::theme::clear_preview(ctx);
                self.persisted_baseline = Some(settings.theme.clone());
                self.draft = settings.theme;
                self.dirty = false;
                self.conflicted = false;
            }
            Err(err) => {
                self.persisted_baseline = None;
                self.conflicted = false;
                self.last_error = Some(format!("Failed to load settings: {err}"));
            }
        }
    }

    pub fn end_session(&mut self, ctx: &egui::Context) {
        crate::gui::theme::clear_preview(ctx);
        self.persisted_baseline = None;
        self.conflicted = false;
        self.dirty = false;
        self.last_error = None;
    }

    pub fn can_edit(&self) -> bool {
        self.persisted_baseline.is_some()
    }

    pub fn can_commit(&self) -> bool {
        self.can_edit() && !self.conflicted
    }

    pub fn is_conflicted(&self) -> bool {
        self.conflicted
    }

    fn effective_scheme(&self, appearance: SystemAppearance) -> ColorScheme {
        effective_color_scheme(&self.draft, appearance)
    }

    fn select_saved_preset(&mut self, name: &str) -> bool {
        if !self.draft.named_presets.contains_key(name) {
            return false;
        }
        if self.draft.mode == ThemeMode::Custom
            && self.draft.preferences.active_preset.as_deref() == Some(name)
        {
            return false;
        }
        self.draft.mode = ThemeMode::Custom;
        self.draft.preferences.active_preset = Some(name.to_owned());
        self.dirty = true;
        true
    }

    fn select_custom_scheme(&mut self) -> bool {
        if self.draft.mode == ThemeMode::Custom && self.draft.preferences.active_preset.is_none() {
            return false;
        }
        self.draft.mode = ThemeMode::Custom;
        self.draft.preferences.active_preset = None;
        self.dirty = true;
        true
    }

    fn select_curated_preset(&mut self, preset_id: &str) -> bool {
        let Some(scheme) = curated_scheme(preset_id) else {
            return false;
        };
        let base_key = format!("curated.{preset_id}");
        let name = self
            .draft
            .named_presets
            .iter()
            .filter(|(_, candidate)| *candidate == &scheme)
            .map(|(name, _)| name)
            .min()
            .cloned()
            .unwrap_or_else(|| {
                let mut candidate = base_key.clone();
                let mut suffix = 2;
                while self
                    .draft
                    .named_presets
                    .get(&candidate)
                    .is_some_and(|existing| existing != &scheme)
                {
                    candidate = format!("{base_key}.{suffix}");
                    suffix += 1;
                }
                candidate
            });
        let inserted = !self.draft.named_presets.contains_key(&name);
        self.draft
            .named_presets
            .entry(name.clone())
            .or_insert(scheme);
        if !inserted
            && self.draft.mode == ThemeMode::Custom
            && self.draft.preferences.active_preset.as_deref() == Some(&name)
        {
            return false;
        }
        self.draft.mode = ThemeMode::Custom;
        self.draft.preferences.active_preset = Some(name);
        self.dirty = true;
        true
    }

    fn apply_manual_scheme(&mut self, scheme: ColorScheme) {
        self.draft.mode = ThemeMode::Custom;
        self.draft.preferences.active_preset = None;
        self.draft.custom_scheme = scheme;
        self.dirty = true;
    }

    pub fn save_to_path(&mut self, ctx: &egui::Context, settings_path: &str) -> Result<(), String> {
        self.last_error = None;
        let Some(baseline) = self.persisted_baseline.clone() else {
            let message = "Reload the saved theme before applying changes.".to_owned();
            self.last_error = Some(message.clone());
            return Err(message);
        };
        if self.conflicted {
            let message =
                "The saved theme changed. Reload it before applying this draft.".to_owned();
            self.last_error = Some(message.clone());
            return Err(message);
        }
        let draft = self.draft.clone();
        let mut conflict = false;
        let result = Settings::update(settings_path, |settings| {
            if settings.theme != baseline {
                conflict = true;
                anyhow::bail!("The saved theme changed while this dialog was open.");
            }
            settings.theme = draft;
            Ok(())
        });
        let committed = match result {
            Ok(committed) => committed,
            Err(err) => {
                let message = if conflict {
                    self.conflicted = true;
                    "The saved theme changed while this dialog was open. Reload the saved theme to continue.".to_owned()
                } else {
                    format!("Failed to save settings: {err}")
                };
                self.last_error = Some(message.clone());
                return Err(message);
            }
        };
        self.persisted_baseline = Some(committed.theme.clone());
        self.draft = committed.theme.clone();
        self.dirty = false;
        self.conflicted = false;
        self.last_error = None;
        crate::gui::theme::publish_committed(ctx, &committed.theme);
        Ok(())
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

    let mut keep_open = *open;
    let mut reload_requested = false;
    let mut draft_changed = false;
    let settings_path = app.settings_path.clone();
    egui::Window::new("Theme settings")
        .open(&mut keep_open)
        .resizable(true)
        .default_width(980.0)
        .show(ctx, |ui| {
            if let Some(err) = &state.last_error {
                widgets::semantic_status(ui, StatusTone::Error, "Error", Some(err));
                ui.separator();
            }

            if !state.can_edit() {
                ui.label(
                    egui::RichText::new(
                        "Theme settings could not be loaded. Retry before editing or saving.",
                    )
                    .text_style(text_style_for_role(TextRole::Body)),
                );
                ui.horizontal(|ui| {
                    if chrome_icon_button(
                        ui,
                        ChromeIcon::Reset,
                        "theme.retry-loading",
                        "Retry loading",
                    )
                    .clicked()
                    {
                        reload_requested = true;
                    }
                    if chrome_icon_button(ui, ChromeIcon::Cancel, "theme.cancel-load", "Cancel")
                        .clicked()
                    {
                        *open = false;
                    }
                });
                return;
            }

            if state.is_conflicted() {
                ui.horizontal(|ui| {
                    widgets::semantic_status(
                        ui,
                        StatusTone::Warning,
                        "Warning",
                        Some("The saved theme changed while this draft was open."),
                    );
                    if chrome_icon_button(
                        ui,
                        ChromeIcon::Reset,
                        "theme.reload-saved",
                        "Reload saved theme",
                    )
                    .clicked()
                    {
                        reload_requested = true;
                    }
                });
                ui.separator();
            }

            widgets::secondary_help(ui, "Customize launcher colors and theme mode.");
            ui.separator();

            let mut changed = false;
            ui.columns(2, |columns| {
                changed |= section_base_mode(&mut columns[0], &mut state.draft.mode);
                columns[0].separator();
                changed |= section_presets(&mut columns[0], state);

                let appearance = system_appearance(ctx);
                let mut scheme = state.effective_scheme(appearance);
                let mode = state.draft.mode;
                let mut scheme_changed = false;
                scheme_changed |= section_core_surfaces(&mut columns[0], &mut scheme, mode);
                columns[0].separator();
                scheme_changed |= section_text_and_links(&mut columns[0], &mut scheme, mode);
                columns[0].separator();
                scheme_changed |= section_widgets(&mut columns[1], &mut scheme, mode);
                columns[1].separator();
                scheme_changed |= section_selection(&mut columns[1], &mut scheme, mode);
                columns[1].separator();
                scheme_changed |= section_semantic(&mut columns[1], &mut scheme, mode);
                if scheme_changed {
                    state.apply_manual_scheme(scheme.clone());
                    changed = true;
                }

                changed |= section_accent(
                    &mut columns[1],
                    &mut state.draft.preferences.accent,
                    scheme.widget_active_fill,
                );
                columns[1].separator();
                preview(&mut columns[1], resolve_theme(&state.draft, appearance));
            });

            if changed {
                state.dirty = true;
                draft_changed = true;
            }

            ui.separator();
            ui.horizontal(|ui| {
                let reset = chrome_icon_button(
                    ui,
                    ChromeIcon::Reset,
                    "theme.reset-all",
                    "Reset all to default",
                );
                if widgets::with_button_emphasis(ui, reset, ButtonEmphasis::Destructive).clicked() {
                    let presets = state.draft.named_presets.clone();
                    state.draft = ThemeSettings::default();
                    state.draft.named_presets = presets;
                    state.dirty = true;
                    draft_changed = true;
                }
                let cancel = chrome_icon_button(ui, ChromeIcon::Cancel, "theme.cancel", "Cancel");
                if widgets::with_button_emphasis(ui, cancel, ButtonEmphasis::Ordinary).clicked() {
                    *open = false;
                }
                if ui
                    .add_enabled_ui(state.dirty && state.can_commit(), |ui| {
                        let apply =
                            chrome_icon_button(ui, ChromeIcon::Apply, "theme.apply", "Apply");
                        widgets::with_button_emphasis(ui, apply, ButtonEmphasis::Ordinary)
                    })
                    .inner
                    .clicked()
                {
                    match state.save_to_path(ctx, &app.settings_path) {
                        Ok(_) => {
                            if app.enable_toasts {
                                app.add_toast(Toast {
                                    text: "Theme settings applied".into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(app.toast_duration as f64),

                                    style: Default::default(),
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

                                    style: Default::default(),
                                });
                            }
                        }
                    }
                }
                if ui
                    .add_enabled_ui(state.dirty && state.can_commit(), |ui| {
                        let save = chrome_icon_button(ui, ChromeIcon::Save, "theme.save", "Save");
                        widgets::with_button_emphasis(ui, save, ButtonEmphasis::Primary)
                    })
                    .inner
                    .clicked()
                {
                    match state.save_to_path(ctx, &app.settings_path) {
                        Ok(_) => {
                            if app.enable_toasts {
                                app.add_toast(Toast {
                                    text: "Theme settings saved".into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(app.toast_duration as f64),

                                    style: Default::default(),
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

                                    style: Default::default(),
                                });
                            }
                        }
                    }
                }
            });
        });

    *open = keep_open && *open;
    if reload_requested {
        state.reload_from_path(ctx, &settings_path);
    } else if draft_changed && *open {
        crate::gui::theme::publish_preview(ctx, &state.draft);
    }
}

fn system_appearance(ctx: &egui::Context) -> SystemAppearance {
    match ctx.theme() {
        egui::Theme::Dark => SystemAppearance::Dark,
        egui::Theme::Light => SystemAppearance::Light,
    }
}

fn section_heading(ui: &mut egui::Ui, title: &str, description: Option<&str>) {
    widgets::compact_section_heading(ui, title, description);
}

fn defaults_for_mode(mode: ThemeMode) -> ColorScheme {
    match mode {
        ThemeMode::Light => ColorScheme::light(),
        ThemeMode::Dark | ThemeMode::Custom | ThemeMode::System => ColorScheme::dark(),
    }
}

fn section_base_mode(ui: &mut egui::Ui, mode: &mut ThemeMode) -> bool {
    let mut changed = false;
    section_heading(
        ui,
        "Base mode",
        Some("Follow the system setting or choose a fixed appearance."),
    );
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

const CURATED_PRESETS: [(&str, &str); 6] = [
    ("graphite-dark", "Graphite · Dark"),
    ("graphite-light", "Graphite · Light"),
    ("slate-dark", "Slate · Dark"),
    ("slate-light", "Slate · Light"),
    ("ocean-dark", "Ocean · Dark"),
    ("ocean-light", "Ocean · Light"),
];

fn section_presets(ui: &mut egui::Ui, state: &mut ThemeSettingsDialogState) -> bool {
    section_heading(
        ui,
        "Palettes",
        Some("Choose a saved palette or start from a curated preset."),
    );
    let active_name = if state.draft.mode == ThemeMode::Custom {
        state
            .draft
            .preferences
            .active_preset
            .as_deref()
            .and_then(|name| state.draft.named_presets.get_key_value(name))
            .map(|(name, _)| name.clone())
    } else {
        None
    };
    let selected_text = active_name
        .as_deref()
        .unwrap_or_else(|| match state.draft.mode {
            ThemeMode::System => "System palette",
            ThemeMode::Dark => "Dark palette",
            ThemeMode::Light => "Light palette",
            ThemeMode::Custom => "Custom colors",
        });
    let mut changed = false;

    egui::ComboBox::from_label("Saved palette")
        .selected_text(selected_text)
        .show_ui(ui, |ui| {
            let custom_selected = state.draft.mode == ThemeMode::Custom && active_name.is_none();
            if ui
                .selectable_label(custom_selected, "Custom colors")
                .clicked()
            {
                changed |= state.select_custom_scheme();
            }
            let mut names: Vec<_> = state.draft.named_presets.keys().collect();
            names.sort_unstable();
            let mut selected_name = None;
            for name in names {
                let selected = state.draft.mode == ThemeMode::Custom
                    && state.draft.preferences.active_preset.as_deref() == Some(name);
                if ui.selectable_label(selected, name).clicked() {
                    selected_name = Some(name.clone());
                }
            }
            if let Some(name) = selected_name {
                changed |= state.select_saved_preset(&name);
            }
        });

    egui::ComboBox::from_label("Curated palette")
        .selected_text("Choose a curated palette")
        .show_ui(ui, |ui| {
            for (id, label) in CURATED_PRESETS {
                if ui.selectable_label(false, label).clicked() {
                    changed |= state.select_curated_preset(id);
                }
            }
        });
    changed
}

fn curated_scheme(id: &str) -> Option<ColorScheme> {
    let mut scheme = match id {
        "graphite-dark" | "slate-dark" | "ocean-dark" => ColorScheme::dark(),
        "graphite-light" | "slate-light" | "ocean-light" => ColorScheme::light(),
        _ => return None,
    };
    match id {
        "graphite-dark" => {
            scheme.window_fill = rgba(27, 29, 33, 255);
            scheme.panel_fill = rgba(35, 37, 42, 255);
            scheme.text = rgba(231, 233, 238, 255);
            scheme.widget_active_fill = rgba(71, 82, 101, 255);
            scheme.widget_active_stroke = rgba(159, 177, 210, 255);
            scheme.selection_bg = rgba(70, 101, 153, 210);
        }
        "graphite-light" => {
            scheme.window_fill = rgba(243, 244, 246, 255);
            scheme.panel_fill = rgba(252, 252, 253, 255);
            scheme.text = rgba(38, 41, 47, 255);
            scheme.widget_active_fill = rgba(211, 217, 228, 255);
            scheme.widget_active_stroke = rgba(91, 105, 130, 255);
            scheme.selection_bg = rgba(169, 187, 219, 220);
        }
        "slate-dark" => {
            scheme.window_fill = rgba(25, 31, 42, 255);
            scheme.panel_fill = rgba(32, 40, 53, 255);
            scheme.text = rgba(225, 233, 244, 255);
            scheme.hyperlink = rgba(130, 180, 237, 255);
            scheme.widget_active_fill = rgba(62, 83, 113, 255);
            scheme.widget_active_stroke = rgba(143, 180, 224, 255);
            scheme.selection_bg = rgba(59, 105, 158, 210);
        }
        "slate-light" => {
            scheme.window_fill = rgba(237, 242, 248, 255);
            scheme.panel_fill = rgba(250, 252, 255, 255);
            scheme.text = rgba(34, 47, 64, 255);
            scheme.hyperlink = rgba(42, 105, 173, 255);
            scheme.widget_active_fill = rgba(197, 214, 235, 255);
            scheme.widget_active_stroke = rgba(75, 111, 153, 255);
            scheme.selection_bg = rgba(146, 185, 224, 220);
        }
        "ocean-dark" => {
            scheme.window_fill = rgba(18, 32, 38, 255);
            scheme.panel_fill = rgba(24, 42, 49, 255);
            scheme.text = rgba(222, 240, 239, 255);
            scheme.hyperlink = rgba(89, 202, 196, 255);
            scheme.widget_active_fill = rgba(38, 104, 105, 255);
            scheme.widget_active_stroke = rgba(111, 211, 199, 255);
            scheme.selection_bg = rgba(37, 123, 122, 215);
        }
        "ocean-light" => {
            scheme.window_fill = rgba(235, 247, 246, 255);
            scheme.panel_fill = rgba(251, 255, 254, 255);
            scheme.text = rgba(27, 55, 55, 255);
            scheme.hyperlink = rgba(20, 123, 120, 255);
            scheme.widget_active_fill = rgba(182, 225, 219, 255);
            scheme.widget_active_stroke = rgba(45, 132, 126, 255);
            scheme.selection_bg = rgba(130, 207, 195, 215);
        }
        _ => return None,
    }
    Some(scheme)
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> ThemeColor {
    ThemeColor { r, g, b, a }
}

fn section_accent(
    ui: &mut egui::Ui,
    accent: &mut Option<ThemeColor>,
    palette_accent: ThemeColor,
) -> bool {
    section_heading(ui, "Accent", None);
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut enabled = accent.is_some();
        if ui.checkbox(&mut enabled, "Override accent").changed() {
            *accent = enabled.then_some(palette_accent);
            changed = true;
        }
        if let Some(accent) = accent.as_mut() {
            changed |= color_row(ui, "Accent color", accent);
        }
    });
    changed
}

fn section_core_surfaces(ui: &mut egui::Ui, scheme: &mut ColorScheme, mode: ThemeMode) -> bool {
    let mut changed = false;
    section_heading(ui, "Core surfaces", None);
    changed |= color_row(ui, "Window background", &mut scheme.window_fill);
    changed |= color_row(ui, "Panel background", &mut scheme.panel_fill);
    if chrome_small_icon_button(
        ui,
        ChromeIcon::Reset,
        "theme.reset.core-surfaces",
        "Reset section defaults",
    )
    .clicked()
    {
        reset_core_surfaces_to_mode_defaults(scheme, mode);
        changed = true;
    }
    changed
}

fn section_text_and_links(ui: &mut egui::Ui, scheme: &mut ColorScheme, mode: ThemeMode) -> bool {
    let mut changed = false;
    section_heading(ui, "Text and links", None);
    changed |= color_row(ui, "Text", &mut scheme.text);
    changed |= color_row(ui, "Hyperlink", &mut scheme.hyperlink);
    if chrome_small_icon_button(
        ui,
        ChromeIcon::Reset,
        "theme.reset.text-and-links",
        "Reset section defaults",
    )
    .clicked()
    {
        reset_text_and_links_to_mode_defaults(scheme, mode);
        changed = true;
    }
    changed
}

fn section_widgets(ui: &mut egui::Ui, scheme: &mut ColorScheme, mode: ThemeMode) -> bool {
    let mut changed = false;
    section_heading(ui, "Interactive widgets", None);
    changed |= color_row(ui, "Inactive fill", &mut scheme.widget_inactive_fill);
    changed |= color_row(ui, "Inactive stroke", &mut scheme.widget_inactive_stroke);
    changed |= color_row(ui, "Hovered fill", &mut scheme.widget_hovered_fill);
    changed |= color_row(ui, "Hovered stroke", &mut scheme.widget_hovered_stroke);
    changed |= color_row(ui, "Active fill", &mut scheme.widget_active_fill);
    changed |= color_row(ui, "Active stroke", &mut scheme.widget_active_stroke);
    if chrome_small_icon_button(
        ui,
        ChromeIcon::Reset,
        "theme.reset.widgets",
        "Reset section defaults",
    )
    .clicked()
    {
        reset_widgets_to_mode_defaults(scheme, mode);
        changed = true;
    }
    changed
}

fn section_selection(ui: &mut egui::Ui, scheme: &mut ColorScheme, mode: ThemeMode) -> bool {
    let mut changed = false;
    section_heading(ui, "Selection and highlight", None);
    changed |= color_row(ui, "Selection background", &mut scheme.selection_bg);
    changed |= color_row(ui, "Selection stroke", &mut scheme.selection_stroke);
    if chrome_small_icon_button(
        ui,
        ChromeIcon::Reset,
        "theme.reset.selection",
        "Reset section defaults",
    )
    .clicked()
    {
        reset_selection_to_mode_defaults(scheme, mode);
        changed = true;
    }
    changed
}

fn section_semantic(ui: &mut egui::Ui, scheme: &mut ColorScheme, mode: ThemeMode) -> bool {
    let mut changed = false;
    section_heading(ui, "Semantic accents", None);
    changed |= color_row(ui, "Success", &mut scheme.success_accent);
    changed |= color_row(ui, "Warning", &mut scheme.warn_accent);
    changed |= color_row(ui, "Error", &mut scheme.error_accent);
    if chrome_small_icon_button(
        ui,
        ChromeIcon::Reset,
        "theme.reset.semantic",
        "Reset section defaults",
    )
    .clicked()
    {
        reset_semantic_to_mode_defaults(scheme, mode);
        changed = true;
    }
    changed
}

fn color_row(ui: &mut egui::Ui, label: &str, color: &mut ThemeColor) -> bool {
    const LABEL_WIDTH: f32 = 150.0;
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_min_height(22.0);
        ui.add_sized([LABEL_WIDTH, 0.0], egui::Label::new(label));
        ui.spacing_mut().item_spacing.x = 6.0;
        let mut rgba = [color.r, color.g, color.b, color.a];
        if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
            [color.r, color.g, color.b, color.a] = rgba;
            changed = true;
        }
    });
    changed
}

fn reset_core_surfaces_to_mode_defaults(scheme: &mut ColorScheme, mode: ThemeMode) {
    let defaults = defaults_for_mode(mode);
    scheme.window_fill = defaults.window_fill;
    scheme.panel_fill = defaults.panel_fill;
}

fn reset_text_and_links_to_mode_defaults(scheme: &mut ColorScheme, mode: ThemeMode) {
    let defaults = defaults_for_mode(mode);
    scheme.text = defaults.text;
    scheme.hyperlink = defaults.hyperlink;
}

fn reset_widgets_to_mode_defaults(scheme: &mut ColorScheme, mode: ThemeMode) {
    let defaults = defaults_for_mode(mode);
    scheme.widget_inactive_fill = defaults.widget_inactive_fill;
    scheme.widget_inactive_stroke = defaults.widget_inactive_stroke;
    scheme.widget_hovered_fill = defaults.widget_hovered_fill;
    scheme.widget_hovered_stroke = defaults.widget_hovered_stroke;
    scheme.widget_active_fill = defaults.widget_active_fill;
    scheme.widget_active_stroke = defaults.widget_active_stroke;
}

fn reset_selection_to_mode_defaults(scheme: &mut ColorScheme, mode: ThemeMode) {
    let defaults = defaults_for_mode(mode);
    scheme.selection_bg = defaults.selection_bg;
    scheme.selection_stroke = defaults.selection_stroke;
}

fn reset_semantic_to_mode_defaults(scheme: &mut ColorScheme, mode: ThemeMode) {
    let defaults = defaults_for_mode(mode);
    scheme.success_accent = defaults.success_accent;
    scheme.warn_accent = defaults.warn_accent;
    scheme.error_accent = defaults.error_accent;
}

fn preview(ui: &mut egui::Ui, resolved: ResolvedTheme) {
    let palette = resolved.palette;
    section_heading(ui, "Live preview", None);
    egui::Frame::NONE
        .fill(to_egui(palette.window_surface))
        .stroke(egui::Stroke::new(1.0_f32, to_egui(palette.control_border)))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Window text")
                    .text_style(text_style_for_role(TextRole::Body))
                    .color(to_egui(palette.primary_text)),
            );
            ui.hyperlink_to(
                egui::RichText::new("Link sample").color(to_egui(palette.hyperlink)),
                "https://example.com",
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                swatch(ui, "Inactive", palette.control_fill, palette.control_border);
                swatch(
                    ui,
                    "Hovered",
                    palette.control_hover_fill,
                    palette.control_hover_border,
                );
                swatch(
                    ui,
                    "Active",
                    palette.control_active_fill,
                    palette.control_active_border,
                );
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                swatch(
                    ui,
                    "Selection",
                    palette.selection_fill,
                    palette.selection_border,
                );
                swatch(ui, "Accent", palette.accent, palette.focus);
                swatch(ui, "Success", palette.success, palette.success);
                swatch(ui, "Warning", palette.warning, palette.warning);
                swatch(ui, "Error", palette.error, palette.error);
            });
        });
}

fn swatch(ui: &mut egui::Ui, label: &str, fill: ThemeColor, stroke: ThemeColor) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(90.0, 26.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        4.0,
        to_egui(fill),
        egui::Stroke::new(1.0_f32, to_egui(stroke)),
        egui::StrokeKind::Middle,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        text_style_for_role(TextRole::Metadata).resolve(ui.style()),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_path(directory: &tempfile::TempDir, settings: &Settings) -> String {
        let path = directory.path().join("settings.json");
        settings.save(path.to_str().unwrap()).unwrap();
        path.to_str().unwrap().to_owned()
    }

    #[test]
    fn defaults_for_mode_returns_expected_scheme() {
        assert_eq!(defaults_for_mode(ThemeMode::System), ColorScheme::dark());
        assert_eq!(defaults_for_mode(ThemeMode::Dark), ColorScheme::dark());
        assert_eq!(defaults_for_mode(ThemeMode::Light), ColorScheme::light());
        assert_eq!(defaults_for_mode(ThemeMode::Custom), ColorScheme::dark());
    }

    #[test]
    fn successful_theme_save_publishes_the_committed_settings() {
        let directory = tempfile::tempdir().unwrap();
        let mut stored = Settings::default();
        stored.theme.mode = ThemeMode::Dark;
        let path = settings_path(&directory, &stored);

        let ctx = egui::Context::default();
        crate::gui::theme::initialize_runtime(&ctx, &stored.theme);
        let mut state = ThemeSettingsDialogState::default();
        state.begin_session(&ctx, &path);
        state.draft.mode = ThemeMode::Light;
        state.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &state.draft);

        state.save_to_path(&ctx, &path).unwrap();

        assert!(!state.dirty);
        assert!(state.can_edit());
        assert_eq!(
            state.persisted_baseline.as_ref().unwrap().mode,
            ThemeMode::Light
        );
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert_eq!(state.draft.mode, ThemeMode::Light);
        assert_eq!(Settings::load(&path).unwrap().theme.mode, ThemeMode::Light);
    }

    #[test]
    fn failed_theme_save_preserves_draft_baseline_and_preview() {
        let directory = tempfile::tempdir().unwrap();
        let mut stored = Settings::default();
        stored.theme.mode = ThemeMode::Dark;
        let path = settings_path(&directory, &stored);
        let ctx = egui::Context::default();
        crate::gui::theme::initialize_runtime(&ctx, &stored.theme);
        let mut state = ThemeSettingsDialogState::default();
        state.begin_session(&ctx, &path);
        let baseline = state.persisted_baseline.clone();
        state.draft.mode = ThemeMode::Light;
        state.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &state.draft);

        // Establish genuine idle after egui's startup repaint, before edits.
        for _ in 0..4 {
            if !ctx.has_requested_repaint_for(&egui::ViewportId::ROOT) {
                break;
            }
            ctx.run_ui(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
        }
        let draft = state.draft.clone();
        // A missing file loads defaults and is a legitimate theme conflict.
        // A directory instead produces an I/O failure before conflict checking.
        let bad_path = directory.path().join("not-a-settings-file");
        std::fs::create_dir(&bad_path).unwrap();

        assert!(
            state
                .save_to_path(&ctx, bad_path.to_str().unwrap())
                .is_err()
        );
        assert!(state.dirty);
        assert_eq!(state.draft, draft);
        assert_eq!(state.persisted_baseline, baseline);
        assert!(state.can_commit());
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert!(ctx.style_of(egui::Theme::Light).visuals.dark_mode == false);
    }

    #[test]
    fn theme_conflicts_retain_draft_reload_explicitly_and_preserve_unrelated_settings() {
        let directory = tempfile::tempdir().unwrap();
        let mut stored = Settings::default();
        stored.theme.mode = ThemeMode::Dark;
        stored.usage_weight = 0.4;
        let path = settings_path(&directory, &stored);
        let ctx = egui::Context::default();
        crate::gui::theme::initialize_runtime(&ctx, &stored.theme);
        let mut state = ThemeSettingsDialogState::default();
        state.begin_session(&ctx, &path);
        state.draft.mode = ThemeMode::Custom;
        state.draft.custom_scheme.window_fill = rgba(41, 82, 123, 255);
        state.dirty = true;
        let draft = state.draft.clone();
        crate::gui::theme::publish_preview(&ctx, &state.draft);

        let external = Settings::update(&path, |settings| {
            settings.theme.mode = ThemeMode::Light;
            settings.usage_weight = 0.91;
            Ok(())
        })
        .unwrap();
        crate::gui::theme::publish_committed(&ctx, &external.theme);
        assert!(state.save_to_path(&ctx, &path).is_err());
        assert!(state.is_conflicted());
        assert!(!state.can_commit());
        assert_eq!(state.draft, draft);
        assert_eq!(Settings::load(&path).unwrap().theme, external.theme);
        assert_eq!(Settings::load(&path).unwrap().usage_weight, 0.91);
        assert_eq!(
            ctx.theme(),
            egui::Theme::Dark,
            "the draft remains previewed"
        );

        state.reload_from_path(&ctx, &path);
        assert!(!state.is_conflicted());
        assert_eq!(state.draft, external.theme);
        assert_eq!(ctx.theme(), egui::Theme::Light);

        state.draft.mode = ThemeMode::Custom;
        state.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &state.draft);
        state.save_to_path(&ctx, &path).unwrap();
        let final_settings = Settings::load(&path).unwrap();
        assert_eq!(final_settings.usage_weight, 0.91);
        assert_eq!(final_settings.theme.mode, ThemeMode::Custom);
    }

    #[test]
    fn committed_updates_under_preview_are_restored_on_cancel_and_system_keeps_following_os() {
        let directory = tempfile::tempdir().unwrap();
        let mut stored = Settings::default();
        stored.theme.mode = ThemeMode::System;
        let path = settings_path(&directory, &stored);
        let ctx = egui::Context::default();
        crate::gui::theme::initialize_runtime(&ctx, &stored.theme);
        let initial_output = ctx.run_ui(
            egui::RawInput {
                system_theme: Some(egui::Theme::Light),
                ..Default::default()
            },
            |_| {},
        );
        initial_output.drop_without_applying_deltas();
        assert_eq!(ctx.theme(), egui::Theme::Light);

        let mut state = ThemeSettingsDialogState::default();
        state.begin_session(&ctx, &path);
        state.draft.preferences.density = crate::settings::ThemeDensity::Comfortable;
        state.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &state.draft);
        assert_eq!(ctx.theme(), egui::Theme::Light);

        let output = ctx.run_ui(
            egui::RawInput {
                system_theme: Some(egui::Theme::Dark),
                ..Default::default()
            },
            |_| {},
        );
        output.drop_without_applying_deltas();
        assert_eq!(ctx.theme(), egui::Theme::Dark);

        let mut external = stored.theme.clone();
        external.mode = ThemeMode::Custom;
        external.custom_scheme.window_fill = rgba(208, 219, 230, 255);
        crate::gui::theme::publish_committed(&ctx, &external);
        assert_eq!(
            ctx.theme(),
            egui::Theme::Dark,
            "preview still wins until cancellation"
        );

        state.end_session(&ctx);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        assert_eq!(
            ctx.style_of(egui::Theme::Dark).visuals.window_fill,
            to_egui(external.custom_scheme.window_fill),
            "the latest committed palette is revealed after cancelling the preview"
        );
        state.end_session(&ctx);
        assert_eq!(ctx.theme(), egui::Theme::Dark, "cancellation is idempotent");
    }

    #[test]
    fn failed_load_cannot_save_stale_theme_and_can_be_retried() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, "{ malformed").unwrap();
        let ctx = egui::Context::default();
        let initial = ThemeSettings::default();
        crate::gui::theme::initialize_runtime(&ctx, &initial);

        let mut state = ThemeSettingsDialogState::default();
        state.draft.mode = ThemeMode::Light;
        state.dirty = true;
        state.begin_session(&ctx, path.to_str().unwrap());
        assert!(!state.can_edit());
        let stale_draft = state.draft.clone();
        assert!(state.save_to_path(&ctx, path.to_str().unwrap()).is_err());
        assert_eq!(state.draft, stale_draft);
        assert!(Settings::load(path.to_str().unwrap()).is_err());

        let recovered = Settings::default();
        recovered.save(path.to_str().unwrap()).unwrap();
        state.reload_from_path(&ctx, path.to_str().unwrap());
        assert!(state.can_edit());
        assert_eq!(state.draft, recovered.theme);
    }

    #[test]
    fn curated_presets_are_collision_safe_and_manual_edits_copy_the_selected_scheme() {
        let directory = tempfile::tempdir().unwrap();
        let mut stored = Settings::default();
        stored.theme.mode = ThemeMode::Custom;
        let user_scheme = ColorScheme {
            window_fill: rgba(12, 34, 56, 255),
            ..ColorScheme::dark()
        };
        stored
            .theme
            .named_presets
            .insert("curated.graphite-dark".into(), user_scheme.clone());
        let path = settings_path(&directory, &stored);
        let ctx = egui::Context::default();
        crate::gui::theme::initialize_runtime(&ctx, &stored.theme);
        let mut state = ThemeSettingsDialogState::default();
        state.begin_session(&ctx, &path);

        assert!(state.select_curated_preset("graphite-dark"));
        let active = state.draft.preferences.active_preset.as_deref().unwrap();
        assert_eq!(active, "curated.graphite-dark.2");
        assert_eq!(
            state.draft.named_presets["curated.graphite-dark"], user_scheme,
            "a user collision remains unchanged"
        );
        let selected = curated_scheme("graphite-dark").unwrap();
        assert_eq!(state.effective_scheme(SystemAppearance::Dark), selected);

        let mut manual = state.effective_scheme(SystemAppearance::Dark);
        manual.window_fill = rgba(151, 92, 33, 83);
        state.apply_manual_scheme(manual.clone());
        assert_eq!(state.draft.mode, ThemeMode::Custom);
        assert_eq!(state.draft.preferences.active_preset, None);
        assert_eq!(state.draft.custom_scheme, manual);
        assert_eq!(manual.panel_fill, selected.panel_fill);
        assert_eq!(
            state.draft.named_presets["curated.graphite-dark"],
            user_scheme
        );

        let mut missing_active = Settings::default();
        missing_active.theme.mode = ThemeMode::Custom;
        missing_active.theme.preferences.active_preset = Some("curated.ocean-dark".into());
        missing_active
            .theme
            .named_presets
            .insert("curated.graphite-dark".into(), user_scheme.clone());
        let missing_active_path = settings_path(&directory, &missing_active);
        let mut missing_active_state = ThemeSettingsDialogState::default();
        missing_active_state.begin_session(&ctx, &missing_active_path);
        assert!(!missing_active_state.dirty);
        assert!(missing_active_state.select_curated_preset("ocean-dark"));
        assert!(missing_active_state.dirty);
        assert_eq!(
            missing_active_state
                .draft
                .preferences
                .active_preset
                .as_deref(),
            Some("curated.ocean-dark")
        );
        assert_eq!(
            missing_active_state.draft.named_presets["curated.ocean-dark"],
            curated_scheme("ocean-dark").unwrap()
        );
        assert_eq!(
            missing_active_state.draft.named_presets["curated.graphite-dark"], user_scheme,
            "selection preserves unrelated saved preset collisions"
        );
        assert_eq!(
            missing_active_state.effective_scheme(SystemAppearance::Dark),
            curated_scheme("ocean-dark").unwrap()
        );

        let before = resolve_theme(&state.draft, SystemAppearance::Dark);
        let hyperlink = before.palette.hyperlink;
        let selection = before.palette.selection_fill;
        state.draft.preferences.accent = Some(rgba(10, 150, 210, 83));
        let after = resolve_theme(&state.draft, SystemAppearance::Dark);
        assert_eq!(after.metrics_identity, before.metrics_identity);
        assert_eq!(after.palette.hyperlink, hyperlink);
        assert_eq!(after.palette.selection_fill, selection);
        assert_eq!(after.palette.accent, rgba(10, 150, 210, 83));
        assert_eq!(after.palette.focus, rgba(10, 150, 210, 83));
    }

    #[test]
    fn rgba_editor_boundary_preserves_translucent_channels_without_edits() {
        let ctx = egui::Context::default();
        let original = rgba(173, 71, 219, 83);
        let mut color = original;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(!color_row(ui, "Translucent color", &mut color));
        });
        output.drop_without_applying_deltas();
        assert_eq!(color, original);
    }

    #[test]
    fn reset_core_surfaces_only_updates_core_surface_fields() {
        let mut scheme = ColorScheme::light();
        let before = scheme.clone();
        reset_core_surfaces_to_mode_defaults(&mut scheme, ThemeMode::Dark);
        let expected = ColorScheme::dark();

        assert_eq!(scheme.window_fill, expected.window_fill);
        assert_eq!(scheme.panel_fill, expected.panel_fill);
        assert_eq!(scheme.text, before.text);
        assert_eq!(scheme.hyperlink, before.hyperlink);
        assert_eq!(scheme.selection_bg, before.selection_bg);
    }

    #[test]
    fn reset_widgets_only_updates_widget_fields() {
        let mut scheme = ColorScheme::light();
        let before = scheme.clone();
        reset_widgets_to_mode_defaults(&mut scheme, ThemeMode::Dark);
        let expected = ColorScheme::dark();

        assert_eq!(scheme.widget_inactive_fill, expected.widget_inactive_fill);
        assert_eq!(
            scheme.widget_inactive_stroke,
            expected.widget_inactive_stroke
        );
        assert_eq!(scheme.widget_hovered_fill, expected.widget_hovered_fill);
        assert_eq!(scheme.widget_hovered_stroke, expected.widget_hovered_stroke);
        assert_eq!(scheme.widget_active_fill, expected.widget_active_fill);
        assert_eq!(scheme.widget_active_stroke, expected.widget_active_stroke);
        assert_eq!(scheme.window_fill, before.window_fill);
        assert_eq!(scheme.selection_bg, before.selection_bg);
        assert_eq!(scheme.success_accent, before.success_accent);
    }

    #[test]
    fn reset_selection_only_updates_selection_fields() {
        let mut scheme = ColorScheme::light();
        let before = scheme.clone();
        reset_selection_to_mode_defaults(&mut scheme, ThemeMode::Dark);
        let expected = ColorScheme::dark();

        assert_eq!(scheme.selection_bg, expected.selection_bg);
        assert_eq!(scheme.selection_stroke, expected.selection_stroke);
        assert_eq!(scheme.text, before.text);
        assert_eq!(scheme.warn_accent, before.warn_accent);
    }

    #[test]
    fn reset_semantic_only_updates_semantic_fields() {
        let mut scheme = ColorScheme::light();
        let before = scheme.clone();
        reset_semantic_to_mode_defaults(&mut scheme, ThemeMode::Dark);
        let expected = ColorScheme::dark();

        assert_eq!(scheme.success_accent, expected.success_accent);
        assert_eq!(scheme.warn_accent, expected.warn_accent);
        assert_eq!(scheme.error_accent, expected.error_accent);
        assert_eq!(scheme.window_fill, before.window_fill);
        assert_eq!(scheme.widget_active_fill, before.widget_active_fill);
    }

    #[test]
    fn reset_text_and_links_only_updates_text_fields() {
        let mut scheme = ColorScheme::light();
        let before = scheme.clone();
        reset_text_and_links_to_mode_defaults(&mut scheme, ThemeMode::Dark);
        let expected = ColorScheme::dark();

        assert_eq!(scheme.text, expected.text);
        assert_eq!(scheme.hyperlink, expected.hyperlink);
        assert_eq!(scheme.panel_fill, before.panel_fill);
        assert_eq!(scheme.selection_stroke, before.selection_stroke);
    }
}
