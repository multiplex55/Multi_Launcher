use crate::gui::theme::presets::{PaletteFamily, curated_presets, find_curated_preset};
use crate::gui::theme::widgets::{self, ButtonEmphasis, StatusTone};
use crate::gui::theme::{
    ChromeIcon, ResolvedTheme, SystemAppearance, TextRole, chrome_icon_button,
    chrome_small_icon_button, effective_color_scheme, resolve_theme, text_style_for_role,
};
use crate::settings::{
    ColorScheme, Settings, ThemeColor, ThemeMode, ThemePreferences, ThemeSettings,
};
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};

#[derive(Debug, Clone)]
pub struct ThemeSettingsDialogState {
    pub draft: ThemeSettings,
    pub dirty: bool,
    pub last_error: Option<String>,
    persisted_baseline: Option<ThemeSettings>,
    conflicted: bool,
    curated_family: PaletteFamily,
    palette_update_target: Option<String>,
    save_palette_name: String,
    #[cfg(test)]
    palette_card_rects: Vec<egui::Rect>,
}

impl Default for ThemeSettingsDialogState {
    fn default() -> Self {
        Self {
            draft: ThemeSettings::default(),
            dirty: false,
            last_error: None,
            persisted_baseline: None,
            conflicted: false,
            curated_family: PaletteFamily::Oled,
            palette_update_target: None,
            save_palette_name: String::new(),
            #[cfg(test)]
            palette_card_rects: Vec::new(),
        }
    }
}

impl ThemeSettingsDialogState {
    pub fn begin_session(&mut self, ctx: &egui::Context, settings_path: &str) {
        self.persisted_baseline = None;
        self.conflicted = false;
        self.dirty = false;
        self.last_error = None;
        self.palette_update_target = None;
        self.save_palette_name.clear();
        self.reload_from_path(ctx, settings_path);
    }

    pub fn reload_from_path(&mut self, ctx: &egui::Context, settings_path: &str) {
        self.last_error = None;
        match Settings::load(settings_path) {
            Ok(settings) => {
                crate::gui::theme::publish_committed(ctx, &settings.theme);
                crate::gui::theme::clear_preview(ctx);
                let persisted_theme = settings.theme;
                let mut draft = persisted_theme.clone();
                draft.inherit_missing_named_preset_typography_scales();
                self.palette_update_target = (draft.mode == ThemeMode::Custom)
                    .then(|| draft.preferences.active_preset.clone())
                    .flatten()
                    .filter(|name| draft.named_presets.contains_key(name));
                self.persisted_baseline = Some(persisted_theme);
                self.draft = draft;
                self.dirty = false;
                self.conflicted = false;
            }
            Err(err) => {
                self.persisted_baseline = None;
                self.conflicted = false;
                self.palette_update_target = None;
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
        self.palette_update_target = None;
        self.save_palette_name.clear();
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
        self.palette_update_target = Some(name.to_owned());
        self.draft.preferences.typography_scale = self.draft.named_preset_typography_scale(name);
        self.dirty = true;
        true
    }

    fn select_custom_scheme(&mut self) -> bool {
        if self.draft.mode == ThemeMode::Custom && self.draft.preferences.active_preset.is_none() {
            self.palette_update_target = None;
            return false;
        }
        self.draft.mode = ThemeMode::Custom;
        self.draft.preferences.active_preset = None;
        self.palette_update_target = None;
        self.dirty = true;
        true
    }

    fn select_curated_preset(&mut self, preset_id: &str) -> bool {
        let Some(preset) = find_curated_preset(preset_id) else {
            return false;
        };
        let scheme = preset.scheme();
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
        let associated_scale = if inserted {
            1.0
        } else {
            self.draft.named_preset_typography_scale(&name)
        };
        self.draft
            .named_presets
            .entry(name.clone())
            .or_insert(scheme);
        if inserted {
            self.draft
                .named_preset_typography_scales
                .insert(name.clone(), associated_scale);
        } else {
            self.draft
                .named_preset_typography_scales
                .entry(name.clone())
                .or_insert(associated_scale);
        }
        if !inserted
            && self.draft.mode == ThemeMode::Custom
            && self.draft.preferences.active_preset.as_deref() == Some(&name)
        {
            return false;
        }
        self.draft.mode = ThemeMode::Custom;
        self.draft.preferences.active_preset = Some(name);
        self.palette_update_target = self.draft.preferences.active_preset.clone();
        self.draft.preferences.typography_scale = associated_scale;
        self.dirty = true;
        true
    }

    fn select_curated_family(&mut self, family: PaletteFamily) -> bool {
        if self.curated_family == family {
            return false;
        }
        self.curated_family = family;
        true
    }

    fn base_mode_changed(&mut self, previous_mode: ThemeMode) {
        if previous_mode == self.draft.mode {
            return;
        }
        if self.draft.mode != ThemeMode::Custom {
            self.palette_update_target = None;
            return;
        }

        let active_name = self
            .draft
            .preferences
            .active_preset
            .clone()
            .filter(|name| self.draft.named_presets.contains_key(name));
        self.draft.preferences.active_preset = active_name.clone();
        self.palette_update_target = active_name.clone();
        if let Some(name) = active_name {
            self.draft.preferences.typography_scale =
                self.draft.named_preset_typography_scale(&name);
        }
    }

    fn apply_manual_scheme(&mut self, scheme: ColorScheme) {
        self.draft.mode = ThemeMode::Custom;
        self.draft.preferences.active_preset = None;
        self.draft.custom_scheme = scheme;
        self.dirty = true;
    }

    fn set_typography_scale(&mut self, scale: f32, appearance: SystemAppearance) -> bool {
        let scale = ThemePreferences::normalize_typography_scale(scale);
        let current_scale = self.draft.preferences.normalized().typography_scale;
        if scale == current_scale && self.draft.preferences.typography_scale == scale {
            return false;
        }

        let was_saved_palette =
            self.draft.mode == ThemeMode::Custom && self.draft.preferences.active_preset.is_some();
        let scheme = self.effective_scheme(appearance);
        self.draft.preferences.typography_scale = scale;
        if was_saved_palette {
            self.draft.preferences.active_preset = None;
            self.draft.custom_scheme = scheme;
        }
        self.dirty = true;
        true
    }

    fn save_as_palette(
        &mut self,
        requested_name: &str,
        appearance: SystemAppearance,
    ) -> Result<(), String> {
        let name = requested_name.trim();
        if name.is_empty() {
            return Err("Enter a name for the new palette.".to_owned());
        }
        if self.draft.named_presets.contains_key(name) {
            return Err(format!("A saved palette named '{name}' already exists."));
        }

        let scheme = self.effective_scheme(appearance);
        let scale = self.draft.preferences.normalized().typography_scale;
        self.draft
            .named_presets
            .insert(name.to_owned(), scheme.clone());
        self.draft
            .named_preset_typography_scales
            .insert(name.to_owned(), scale);
        self.draft.mode = ThemeMode::Custom;
        self.draft.custom_scheme = scheme;
        self.draft.preferences.active_preset = Some(name.to_owned());
        self.draft.preferences.typography_scale = scale;
        self.palette_update_target = Some(name.to_owned());
        self.save_palette_name = name.to_owned();
        self.dirty = true;
        self.last_error = None;
        Ok(())
    }

    fn update_palette(&mut self, appearance: SystemAppearance) -> Result<bool, String> {
        let Some(name) = self.palette_update_target.clone() else {
            return Err("Select a saved palette before updating it.".to_owned());
        };
        if !self.draft.named_presets.contains_key(&name) {
            return Err(format!("The saved palette '{name}' no longer exists."));
        }

        let scheme = self.effective_scheme(appearance);
        let scale = self.draft.preferences.normalized().typography_scale;
        let unchanged = self.draft.named_presets.get(&name) == Some(&scheme)
            && self
                .draft
                .named_preset_typography_scales
                .get(&name)
                .is_some_and(|existing| {
                    ThemePreferences::normalize_typography_scale(*existing) == scale
                });
        if unchanged {
            return Ok(false);
        }

        self.draft
            .named_presets
            .insert(name.clone(), scheme.clone());
        self.draft
            .named_preset_typography_scales
            .insert(name.clone(), scale);
        self.draft.mode = ThemeMode::Custom;
        self.draft.custom_scheme = scheme;
        self.draft.preferences.active_preset = Some(name);
        self.draft.preferences.typography_scale = scale;
        self.dirty = true;
        self.last_error = None;
        Ok(true)
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
        let mut draft = self.draft.clone();
        draft.normalize_typography_scales_for_save();
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
        .max_width(ctx.content_rect().width())
        .max_height(ctx.content_rect().height())
        .vscroll(false)
        .default_width(980.0)
        .default_height(theme_window_initial_height(ctx.content_rect().height()))
        .show(ctx, |ui| {
            if !state.can_edit() {
                if let Some(err) = &state.last_error {
                    widgets::semantic_status(ui, StatusTone::Error, "Error", Some(err));
                    ui.separator();
                }
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

            let footer_layout = theme_footer_layout(ui);
            let body_max_height =
                theme_content_height(ui.available_height(), footer_layout.reserved_height);
            let mut changed = false;
            egui::ScrollArea::vertical()
                .min_scrolled_height(0.0)
                .max_height(body_max_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let (body_changed, body_reload_requested, _) =
                        render_theme_body(ui, state, system_appearance(ctx));
                    changed = body_changed;
                    reload_requested = body_reload_requested;
                });

            if changed {
                state.dirty = true;
                draft_changed = true;
            }

            let (footer_action, _) = render_theme_footer(ui, state, &footer_layout);
            if footer_action == Some(ThemeFooterAction::Reset) {
                reset_draft_preserving_saved_palettes(state);
                draft_changed = true;
            }
            if footer_action == Some(ThemeFooterAction::Cancel) {
                *open = false;
            }
            if footer_action == Some(ThemeFooterAction::Apply) {
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
            if footer_action == Some(ThemeFooterAction::Save) {
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

    *open = keep_open && *open;
    if reload_requested {
        state.reload_from_path(ctx, &settings_path);
    } else if draft_changed && *open {
        crate::gui::theme::publish_preview(ctx, &state.draft);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ThemeFooterAction {
    Reset,
    Cancel,
    Apply,
    Save,
}

impl ThemeFooterAction {
    const ALL: [Self; 4] = [Self::Reset, Self::Cancel, Self::Apply, Self::Save];

    const fn label(self) -> &'static str {
        match self {
            Self::Reset => "Reset all to default",
            Self::Cancel => "Cancel",
            Self::Apply => "Apply",
            Self::Save => "Save",
        }
    }
}

fn render_theme_footer(
    ui: &mut egui::Ui,
    state: &ThemeSettingsDialogState,
    layout: &ThemeFooterLayout,
) -> (Option<ThemeFooterAction>, [egui::Rect; 4]) {
    let mut action = None;
    let mut action_rects = [egui::Rect::NOTHING; 4];
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        // Keep each button's label at its measured, unwrapped width. The
        // wrapped row then moves whole buttons onto the next line, matching
        // the widths used to build this shared footer layout plan.
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        for (index, footer_action) in ThemeFooterAction::ALL.into_iter().enumerate() {
            if layout.break_before[index] {
                // Keep all buttons in this one UI scope so their automatic
                // and icon IDs remain stable as the row plan changes.
                ui.end_row();
            }

            let response = match footer_action {
                ThemeFooterAction::Reset => {
                    let response = chrome_icon_button(
                        ui,
                        ChromeIcon::Reset,
                        "theme.reset-all",
                        "Reset all to default",
                    );
                    widgets::with_button_emphasis(ui, response, ButtonEmphasis::Destructive)
                }
                ThemeFooterAction::Cancel => {
                    let response =
                        chrome_icon_button(ui, ChromeIcon::Cancel, "theme.cancel", "Cancel");
                    widgets::with_button_emphasis(ui, response, ButtonEmphasis::Ordinary)
                }
                ThemeFooterAction::Apply => {
                    ui.add_enabled_ui(state.dirty && state.can_commit(), |ui| {
                        let response =
                            chrome_icon_button(ui, ChromeIcon::Apply, "theme.apply", "Apply");
                        widgets::with_button_emphasis(ui, response, ButtonEmphasis::Ordinary)
                    })
                    .inner
                }
                ThemeFooterAction::Save => {
                    ui.add_enabled_ui(state.dirty && state.can_commit(), |ui| {
                        let response =
                            chrome_icon_button(ui, ChromeIcon::Save, "theme.save", "Save");
                        widgets::with_button_emphasis(ui, response, ButtonEmphasis::Primary)
                    })
                    .inner
                }
            };
            action_rects[index] = response.rect;
            if response.clicked() {
                action = Some(footer_action);
            }
        }
    });
    (action, action_rects)
}

fn render_theme_body(
    ui: &mut egui::Ui,
    state: &mut ThemeSettingsDialogState,
    appearance: SystemAppearance,
) -> (bool, bool, [egui::Rect; 3]) {
    let mut notice_rects = [egui::Rect::NOTHING; 3];
    if let Some(err) = &state.last_error {
        notice_rects[0] = widgets::semantic_status(ui, StatusTone::Error, "Error", Some(err)).rect;
        ui.separator();
    }

    let mut reload_requested = false;
    if state.is_conflicted() {
        let (warning_rect, reload) = ui
            .horizontal_wrapped(|ui| {
                let warning = widgets::semantic_status(
                    ui,
                    StatusTone::Warning,
                    "Warning",
                    Some("The saved theme changed while this draft was open."),
                );
                let reload = chrome_icon_button(
                    ui,
                    ChromeIcon::Reset,
                    "theme.reload-saved",
                    "Reload saved theme",
                );
                (warning.rect, reload)
            })
            .inner;
        notice_rects[1] = warning_rect;
        notice_rects[2] = reload.rect;
        reload_requested = reload.clicked();
        ui.separator();
    }

    widgets::secondary_help(ui, "Customize launcher colors and theme mode.");
    ui.separator();
    let changed = render_theme_sections(ui, state, appearance);
    (changed, reload_requested, notice_rects)
}

const THEME_TWO_COLUMN_MIN_WIDTH: f32 = 760.0;

fn theme_editor_uses_two_columns(available_width: f32) -> bool {
    available_width >= THEME_TWO_COLUMN_MIN_WIDTH
}

fn theme_content_height(available_height: f32, footer_reserved_height: f32) -> f32 {
    (available_height - footer_reserved_height).max(0.0)
}

const THEME_WINDOW_INITIAL_HEIGHT_FRACTION: f32 = 0.85;
const THEME_WINDOW_MAX_INITIAL_HEIGHT: f32 = 720.0;

fn theme_window_initial_height(content_height: f32) -> f32 {
    let content_height = content_height.max(0.0);
    (content_height * THEME_WINDOW_INITIAL_HEIGHT_FRACTION)
        .min(THEME_WINDOW_MAX_INITIAL_HEIGHT)
        .min(content_height)
}

#[derive(Clone, Copy)]
struct ThemeFooterLayout {
    break_before: [bool; 4],
    reserved_height: f32,
}

fn theme_footer_layout(ui: &egui::Ui) -> ThemeFooterLayout {
    let spacing = ui.spacing();
    let style = ui.style();
    let button_font = style
        .override_font_id
        .clone()
        .unwrap_or_else(|| text_style_for_role(TextRole::Button).resolve(style));
    let button_frame_margin = style
        .button_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Inactive,
        )
        .frame
        .total_margin()
        .sum();
    let icon_width = spacing.icon_width.max(12.0);
    let button_sizes = ThemeFooterAction::ALL.map(|action| {
        let text_size = ui
            .painter()
            .layout_no_wrap(
                action.label().to_owned(),
                button_font.clone(),
                ui.visuals().text_color(),
            )
            .size();
        egui::vec2(
            text_size.x + icon_width + spacing.icon_spacing + button_frame_margin.x,
            (text_size.y.max(icon_width) + button_frame_margin.y).max(spacing.interact_size.y),
        )
    });

    let available_width = ui.available_width().max(1.0);
    let mut break_before = [false; ThemeFooterAction::ALL.len()];
    let mut row_width = 0.0;
    for (index, button_size) in button_sizes.iter().enumerate() {
        let button_width = button_size.x;
        let gap = if row_width == 0.0 {
            0.0
        } else {
            spacing.item_spacing.x
        };
        if row_width + gap + button_width > available_width && row_width > 0.0 {
            break_before[index] = true;
            row_width = button_width;
        } else {
            row_width += gap + button_width;
        }
    }

    let mut action_height = 0.0;
    let mut row_height = 0.0f32;
    for (index, button_size) in button_sizes.iter().enumerate() {
        if break_before[index] {
            action_height += row_height + spacing.item_spacing.y;
            row_height = 0.0;
        }
        row_height = row_height.max(button_size.y);
    }
    action_height += row_height.max(spacing.interact_size.y);
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    ThemeFooterLayout {
        break_before,
        reserved_height: action_height + 2.0 * spacing.item_spacing.y + separator_height,
    }
}

fn render_theme_sections(
    ui: &mut egui::Ui,
    state: &mut ThemeSettingsDialogState,
    appearance: SystemAppearance,
) -> bool {
    if theme_editor_uses_two_columns(ui.available_width()) {
        let mut changed = false;
        ui.columns(2, |columns| {
            let (primary_changed, scheme, mode, scheme_changed) =
                render_theme_primary_sections(&mut columns[0], state, appearance);
            changed = primary_changed
                | render_theme_secondary_sections(
                    &mut columns[1],
                    state,
                    appearance,
                    scheme,
                    mode,
                    scheme_changed,
                );
        });
        changed
    } else {
        let (mut changed, scheme, mode, scheme_changed) =
            render_theme_primary_sections(ui, state, appearance);
        changed |=
            render_theme_secondary_sections(ui, state, appearance, scheme, mode, scheme_changed);
        changed
    }
}

fn render_theme_primary_sections(
    ui: &mut egui::Ui,
    state: &mut ThemeSettingsDialogState,
    appearance: SystemAppearance,
) -> (bool, ColorScheme, ThemeMode, bool) {
    let previous_mode = state.draft.mode;
    let mut changed = section_base_mode(ui, &mut state.draft.mode);
    if changed {
        state.base_mode_changed(previous_mode);
    }
    ui.separator();
    changed |= section_presets(ui, state);

    let mut scheme = state.effective_scheme(appearance);
    let mode = state.draft.mode;
    let mut scheme_changed = section_core_surfaces(ui, &mut scheme, mode);
    ui.separator();
    let mut typography_scale =
        ThemePreferences::normalize_typography_scale(state.draft.preferences.typography_scale);
    let (text_scheme_changed, typography_scale_changed) =
        section_text_and_links(ui, &mut scheme, mode, &mut typography_scale);
    scheme_changed |= text_scheme_changed;
    if typography_scale_changed {
        changed |= state.set_typography_scale(typography_scale, appearance);
    }
    changed |= scheme_changed;
    (changed, scheme, mode, scheme_changed)
}

fn render_theme_secondary_sections(
    ui: &mut egui::Ui,
    state: &mut ThemeSettingsDialogState,
    appearance: SystemAppearance,
    mut scheme: ColorScheme,
    mode: ThemeMode,
    mut scheme_changed: bool,
) -> bool {
    scheme_changed |= section_widgets(ui, &mut scheme, mode);
    ui.separator();
    scheme_changed |= section_selection(ui, &mut scheme, mode);
    ui.separator();
    scheme_changed |= section_semantic(ui, &mut scheme, mode);

    let mut changed = scheme_changed;
    let accent_default = scheme.widget_active_fill;
    if scheme_changed {
        state.apply_manual_scheme(scheme);
    }
    changed |= section_accent(ui, &mut state.draft.preferences.accent, accent_default);
    ui.separator();
    preview(ui, resolve_theme(&state.draft, appearance));
    changed
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
    ui.horizontal_wrapped(|ui| {
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
    let selected_text = active_name.as_deref().unwrap_or(match state.draft.mode {
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

    if let Some(target) = state.palette_update_target.clone() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Update target: {target}"));
            if ui.button("Update Palette").clicked() {
                match state.update_palette(system_appearance(ui.ctx())) {
                    Ok(updated) => changed |= updated,
                    Err(err) => state.last_error = Some(err),
                }
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Save As Palette");
        ui.add(
            egui::TextEdit::singleline(&mut state.save_palette_name)
                .id_salt("theme.save-as-palette.name")
                .desired_width(150.0)
                .hint_text("Palette name"),
        );
        if ui.button("Save As").clicked() {
            let name = state.save_palette_name.clone();
            match state.save_as_palette(&name, system_appearance(ui.ctx())) {
                Ok(()) => changed = true,
                Err(err) => state.last_error = Some(err),
            }
        }
    });

    let mut selected_family = state.curated_family;
    egui::ComboBox::from_label("Curated group")
        .selected_text(selected_family.label())
        .show_ui(ui, |ui| {
            for family in PaletteFamily::ALL {
                ui.selectable_value(&mut selected_family, family, family.label());
            }
        });
    state.select_curated_family(selected_family);

    let family_presets: Vec<_> = curated_presets()
        .iter()
        .filter(|preset| preset.family == state.curated_family)
        .collect();
    #[cfg(test)]
    state.palette_card_rects.clear();
    ui.label(
        egui::RichText::new(format!("{} palettes", family_presets.len()))
            .text_style(text_style_for_role(TextRole::Metadata))
            .color(ui.visuals().weak_text_color()),
    );

    let active_scheme = (state.draft.mode == ThemeMode::Custom)
        .then_some(state.draft.preferences.active_preset.as_deref())
        .flatten()
        .and_then(|name| state.draft.named_presets.get(name));
    let mut chosen_preset = None;
    ui.columns(2, |columns| {
        for (index, preset) in family_presets.iter().enumerate() {
            let column = &mut columns[index % 2];
            column.push_id(preset.id, |ui| {
                let scheme = preset.scheme();
                let selected = active_scheme.is_some_and(|active| active == &scheme);
                let response = ui.add_sized(
                    egui::vec2(ui.available_width(), 30.0),
                    egui::Button::selectable(
                        selected,
                        egui::RichText::new(preset.label)
                            .text_style(text_style_for_role(TextRole::Metadata)),
                    ),
                );
                #[cfg(test)]
                state.palette_card_rects.push(response.rect);
                if response.clicked() {
                    chosen_preset = Some(preset.id);
                }
                response.on_hover_text(format!("{} · {}", preset.family.label(), preset.label));
                ui.horizontal(|ui| {
                    let swatch_width = 26.0;
                    let swatch_gap = ui.spacing().item_spacing.x;
                    let swatch_row_width = 3.0 * swatch_width + 2.0 * swatch_gap;
                    ui.add_space(((ui.available_width() - swatch_row_width).max(0.0)) * 0.5);
                    palette_swatch(ui, "Window", scheme.window_fill, scheme.text);
                    palette_swatch(ui, "Panel", scheme.panel_fill, scheme.text);
                    palette_swatch(ui, "Link", scheme.hyperlink, scheme.text);
                });
                ui.add_space(3.0);
            });
        }
    });
    if let Some(id) = chosen_preset {
        changed |= state.select_curated_preset(id);
    }
    changed
}

fn palette_swatch(ui: &mut egui::Ui, name: &str, fill: ThemeColor, border: ThemeColor) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 11.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        2.0,
        to_egui(fill),
        egui::Stroke::new(1.0, to_egui(border)),
        egui::StrokeKind::Inside,
    );
    response.on_hover_text(name);
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
    ui.horizontal_wrapped(|ui| {
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

fn section_text_and_links(
    ui: &mut egui::Ui,
    scheme: &mut ColorScheme,
    mode: ThemeMode,
    typography_scale: &mut f32,
) -> (bool, bool) {
    let mut scheme_changed = false;
    section_heading(ui, "Text and links", None);
    scheme_changed |= color_row(ui, "Text", &mut scheme.text);
    scheme_changed |= color_row(ui, "Hyperlink", &mut scheme.hyperlink);
    let mut scale_changed = typography_scale_controls(ui, typography_scale).0;
    if chrome_small_icon_button(
        ui,
        ChromeIcon::Reset,
        "theme.reset.text-and-links",
        "Reset section defaults",
    )
    .clicked()
    {
        reset_text_and_links_to_mode_defaults(scheme, mode);
        *typography_scale = 1.0;
        scheme_changed = true;
        scale_changed = true;
    }
    (scheme_changed, scale_changed)
}

fn typography_scale_controls(
    ui: &mut egui::Ui,
    typography_scale: &mut f32,
) -> (bool, [egui::Rect; 3]) {
    let mut changed = false;
    let mut rects = [egui::Rect::NOTHING; 3];
    let slider_height = ui
        .spacing()
        .interact_size
        .y
        .max(text_style_for_role(TextRole::Body).resolve(ui.style()).size + 8.0);
    let slider = ui.add_sized(
        [ui.available_width(), slider_height],
        egui::Slider::new(
            typography_scale,
            ThemePreferences::MIN_TYPOGRAPHY_SCALE..=ThemePreferences::MAX_TYPOGRAPHY_SCALE,
        )
        .text("Overall Text Size")
        .step_by(0.05)
        .show_value(false),
    );
    rects[0] = slider.rect;
    changed |= slider.changed();
    ui.horizontal_wrapped(|ui| {
        rects[1] = ui.label(format!("{:.0}%", *typography_scale * 100.0)).rect;
        let reset = ui.button("Reset to 100%");
        rects[2] = reset.rect;
        if reset.clicked() {
            *typography_scale = 1.0;
            changed = true;
        }
    });
    (changed, rects)
}

fn reset_draft_preserving_saved_palettes(state: &mut ThemeSettingsDialogState) {
    let presets = state.draft.named_presets.clone();
    let preset_scales = state.draft.named_preset_typography_scales.clone();
    state.draft = ThemeSettings::default();
    state.draft.named_presets = presets;
    state.draft.named_preset_typography_scales = preset_scales;
    state.palette_update_target = None;
    state.save_palette_name.clear();
    state.dirty = true;
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
    let label_width = (ui.available_width() * 0.42).clamp(92.0, 150.0);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_min_height(22.0);
        ui.add_sized([label_width, 0.0], egui::Label::new(label));
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
            ui.horizontal_wrapped(|ui| {
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
            ui.horizontal_wrapped(|ui| {
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
    fn theme_editor_reflows_without_mutating_the_draft_and_keeps_actions_visible() {
        assert!(theme_editor_uses_two_columns(THEME_TWO_COLUMN_MIN_WIDTH));
        assert!(!theme_editor_uses_two_columns(
            THEME_TWO_COLUMN_MIN_WIDTH - 1.0
        ));

        let ctx = egui::Context::default();
        let mut state = ThemeSettingsDialogState::default();
        let original_draft = state.draft.clone();
        for (width, two_columns) in [(640.0, false), (900.0, true)] {
            let mut changed = false;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 460.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    ui.set_width(width);
                    assert_eq!(
                        theme_editor_uses_two_columns(ui.available_width()),
                        two_columns
                    );
                    changed = render_theme_sections(ui, &mut state, SystemAppearance::Dark);
                },
            );
            output.drop_without_applying_deltas();
            assert!(!changed, "presentation alone does not edit the draft");
            assert_eq!(state.draft, original_draft);
            assert!(!state.dirty);
        }

        let ctx = egui::Context::default();
        let tall_screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 800.0));
        let mut state = ThemeSettingsDialogState::default();
        let mut open = true;
        let mut body_clip = egui::Rect::NOTHING;
        let mut window_rect = egui::Rect::NOTHING;
        for _ in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(tall_screen),
                    ..Default::default()
                },
                |_| {
                    let rendered_window = egui::Window::new("Theme settings")
                        .open(&mut open)
                        .resizable(true)
                        .max_width(ctx.content_rect().width())
                        .max_height(ctx.content_rect().height())
                        .vscroll(false)
                        .default_width(980.0)
                        .default_height(theme_window_initial_height(ctx.content_rect().height()))
                        .show(&ctx, |ui| {
                            let footer_layout = theme_footer_layout(ui);
                            let body_height = theme_content_height(
                                ui.available_height(),
                                footer_layout.reserved_height,
                            );
                            egui::ScrollArea::vertical()
                                .min_scrolled_height(0.0)
                                .max_height(body_height)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    body_clip = ui.clip_rect();
                                    render_theme_body(ui, &mut state, SystemAppearance::Dark);
                                });
                            render_theme_footer(ui, &state, &footer_layout);
                        });
                    window_rect = rendered_window
                        .map(|window| window.response.rect)
                        .unwrap_or(egui::Rect::NOTHING);
                },
            );
            output.drop_without_applying_deltas();
        }
        let mut visible_palette_rows: Vec<_> = state
            .palette_card_rects
            .iter()
            .filter(|rect| body_clip.contains_rect(**rect))
            .map(|rect| rect.center().y)
            .collect();
        visible_palette_rows.sort_by(f32::total_cmp);
        visible_palette_rows.dedup_by(|left, right| (*left - *right).abs() < 1.0);
        assert!(
            window_rect.height() >= theme_window_initial_height(tall_screen.height()) - 8.0,
            "the initial dialog should use the tall viewport instead of its content-sized height; \
             window={window_rect:?}"
        );
        assert!(
            visible_palette_rows.len() >= 3,
            "an ordinary tall viewport should expose several usable palette rows; \
             rows={visible_palette_rows:?}, body_clip={body_clip:?}, \
             cards={:?}, window={window_rect:?}",
            state.palette_card_rects
        );
        assert!(open, "the dialog remains open during layout-only frames");
        assert_eq!(theme_window_initial_height(800.0), 680.0);
        assert_eq!(theme_window_initial_height(1200.0), 720.0);
        assert_eq!(theme_window_initial_height(600.0), 510.0);

        let ctx = egui::Context::default();
        ctx.set_theme(egui::Theme::Dark);
        let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
        style.override_font_id = Some(egui::FontId::proportional(31.0));
        ctx.set_style_of(egui::Theme::Dark, style);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 600.0));
        let mut state = ThemeSettingsDialogState::default();
        state.persisted_baseline = Some(state.draft.clone());
        state.conflicted = true;
        state.last_error = Some("Failed to save settings: access denied.".to_owned());
        let mut open = true;
        let mut action_rects = [egui::Rect::NOTHING; 4];
        let mut notice_rects = [egui::Rect::NOTHING; 3];
        let mut body_clip = egui::Rect::NOTHING;
        let mut window_rect = egui::Rect::NOTHING;
        for _ in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |_| {
                    let rendered_window = egui::Window::new("Theme settings")
                        .open(&mut open)
                        .resizable(true)
                        .max_width(ctx.content_rect().width())
                        .max_height(ctx.content_rect().height())
                        .vscroll(false)
                        .default_width(420.0)
                        .default_height(theme_window_initial_height(ctx.content_rect().height()))
                        .show(&ctx, |ui| {
                            let footer_layout = theme_footer_layout(ui);
                            let body_height = theme_content_height(
                                ui.available_height(),
                                footer_layout.reserved_height,
                            );
                            egui::ScrollArea::vertical()
                                .min_scrolled_height(0.0)
                                .max_height(body_height)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    body_clip = ui.clip_rect();
                                    let (_, _, notices) =
                                        render_theme_body(ui, &mut state, SystemAppearance::Dark);
                                    notice_rects = notices;
                                });
                            let (_, rects) = render_theme_footer(ui, &state, &footer_layout);
                            action_rects = rects;
                        });
                    window_rect = rendered_window
                        .map(|window| window.response.rect)
                        .unwrap_or(egui::Rect::NOTHING);
                },
            );
            output.drop_without_applying_deltas();
        }
        assert!(open, "the dialog remains open during layout-only frames");
        assert!(
            action_rects.iter().all(|rect| screen.contains_rect(*rect)),
            "all theme actions stay visible inside the actual constrained window; \
             actions={action_rects:?}, screen={screen:?}, window={window_rect:?}, \
             body_clip={body_clip:?}, notices={notice_rects:?}"
        );
        let footer_top = action_rects
            .iter()
            .map(|rect| rect.min.y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            notice_rects.iter().all(|rect| {
                rect.is_positive() && body_clip.intersects(*rect) && rect.min.y < footer_top
            }),
            "error, conflict warning, and reload stay in the scrollable body before the footer; \
             notices={notice_rects:?}, body_clip={body_clip:?}, footer_top={footer_top}"
        );
        assert_eq!(state.draft, ThemeSettings::default());
        assert!(!state.dirty);
    }

    #[test]
    fn overall_text_size_controls_wrap_inside_narrow_and_two_column_widths_at_200_percent() {
        for width in [386.0, 450.0] {
            let ctx = egui::Context::default();
            let scaled = ThemeSettings {
                mode: ThemeMode::Dark,
                preferences: ThemePreferences {
                    typography_scale: 2.0,
                    ..ThemePreferences::default()
                },
                ..ThemeSettings::default()
            };
            crate::gui::theme::initialize_runtime(&ctx, &scaled);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 240.0));
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| {
                    ui.set_width(width);
                    let bounds = ui.clip_rect();
                    let mut scale = 2.0;
                    let (changed, rects) = typography_scale_controls(ui, &mut scale);
                    assert!(!changed);
                    assert_eq!(scale, 2.0);
                    assert!(
                        rects.iter().all(|rect| rect.is_positive() && bounds.contains_rect(*rect)),
                        "all text-size controls fit at {width} points and 200%; controls={rects:?}, bounds={bounds:?}"
                    );
                    for first in 0..rects.len() {
                        for second in first + 1..rects.len() {
                            assert!(
                                !rects[first].intersects(rects[second]),
                                "text-size controls do not overlap; controls={rects:?}"
                            );
                        }
                    }
                },
            );
            output.drop_without_applying_deltas();
        }
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
    fn committed_updates_under_preview_restore_system_or_custom_appearance_on_cancel() {
        let directory = tempfile::tempdir().unwrap();
        let mut stored = Settings::default();
        stored.theme.mode = ThemeMode::System;
        stored.theme.preferences.typography_scale = 1.25;
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
        state.draft.preferences.typography_scale = 1.5;
        state.dirty = true;
        crate::gui::theme::publish_preview(&ctx, &state.draft);
        assert_eq!(ctx.theme(), egui::Theme::Light);

        state.draft.preferences.typography_scale = 0.85;
        crate::gui::theme::publish_preview(&ctx, &state.draft);
        assert_eq!(
            ctx.style_of(egui::Theme::Light).text_styles[&egui::TextStyle::Body].size,
            14.0 * 0.85,
            "each preview replaces metrics from the installed baseline"
        );

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
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert_eq!(
            ctx.style_of(egui::Theme::Light).visuals.window_fill,
            to_egui(external.custom_scheme.window_fill),
            "the latest committed palette is revealed after cancelling the preview"
        );
        assert_eq!(
            ctx.style_of(egui::Theme::Light).text_styles[&egui::TextStyle::Body].size,
            14.0 * 1.25,
            "cancelling restores the latest committed typography metrics"
        );
        state.end_session(&ctx);
        assert_eq!(
            ctx.theme(),
            egui::Theme::Light,
            "cancellation is idempotent"
        );
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
        let second_collision = ColorScheme {
            panel_fill: rgba(19, 29, 39, 255),
            ..ColorScheme::dark()
        };
        stored
            .theme
            .named_presets
            .insert("curated.graphite-dark.2".into(), second_collision.clone());
        let path = settings_path(&directory, &stored);
        let ctx = egui::Context::default();
        crate::gui::theme::initialize_runtime(&ctx, &stored.theme);
        let mut state = ThemeSettingsDialogState::default();
        state.begin_session(&ctx, &path);
        state.draft.preferences.typography_scale = 1.6;

        assert!(state.select_curated_preset("graphite-dark"));
        let active = state.draft.preferences.active_preset.as_deref().unwrap();
        assert_eq!(active, "curated.graphite-dark.3");
        assert_eq!(
            state.draft.named_presets["curated.graphite-dark"], user_scheme,
            "a user collision remains unchanged"
        );
        assert_eq!(
            state.draft.named_presets["curated.graphite-dark.2"], second_collision,
            "an occupied suffix also remains unchanged"
        );
        let selected = find_curated_preset("graphite-dark").unwrap().scheme();
        assert_eq!(state.effective_scheme(SystemAppearance::Dark), selected);
        assert_eq!(state.draft.preferences.typography_scale, 1.0);
        let palette_count = state.draft.named_presets.len();
        assert!(!state.select_curated_preset("graphite-dark"));
        assert_eq!(state.draft.named_presets.len(), palette_count);

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
            find_curated_preset("ocean-dark").unwrap().scheme()
        );
        assert_eq!(
            missing_active_state.draft.named_presets["curated.graphite-dark"], user_scheme,
            "selection preserves unrelated saved preset collisions"
        );
        assert_eq!(
            missing_active_state.effective_scheme(SystemAppearance::Dark),
            find_curated_preset("ocean-dark").unwrap().scheme()
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
    fn browsing_curated_families_does_not_mutate_the_theme_draft() {
        let mut state = ThemeSettingsDialogState::default();
        state.draft.named_presets.insert(
            "User palette".into(),
            ColorScheme {
                window_fill: rgba(72, 31, 101, 255),
                ..ColorScheme::dark()
            },
        );
        let draft = state.draft.clone();

        assert!(state.select_curated_family(PaletteFamily::Light));
        assert_eq!(state.curated_family, PaletteFamily::Light);
        assert_eq!(state.draft, draft);
        assert!(!state.dirty);
        assert!(!state.select_curated_family(PaletteFamily::Light));
        assert_eq!(state.draft, draft);
        assert!(!state.dirty);
    }

    #[test]
    fn curated_selection_reuses_equal_saved_scheme_without_adding_a_copy() {
        let directory = tempfile::tempdir().unwrap();
        let scheme = find_curated_preset("obsidian-amber").unwrap().scheme();
        let mut stored = Settings::default();
        stored
            .theme
            .named_presets
            .insert("Amber user copy".into(), scheme.clone());
        stored
            .theme
            .named_presets
            .insert("Amber alternate".into(), scheme.clone());
        stored
            .theme
            .named_preset_typography_scales
            .insert("Amber user copy".into(), 0.85);
        stored
            .theme
            .named_preset_typography_scales
            .insert("Amber alternate".into(), 1.4);
        let path = settings_path(&directory, &stored);
        let ctx = egui::Context::default();
        crate::gui::theme::initialize_runtime(&ctx, &stored.theme);
        let mut state = ThemeSettingsDialogState::default();
        state.begin_session(&ctx, &path);
        let count = state.draft.named_presets.len();

        assert!(state.select_curated_preset("obsidian-amber"));
        assert_eq!(state.draft.named_presets.len(), count);
        assert_eq!(
            state.draft.preferences.active_preset.as_deref(),
            Some("Amber alternate"),
            "equal saved schemes reuse the lexicographically first matching entry"
        );
        assert_eq!(state.draft.preferences.typography_scale, 1.4);
        assert_eq!(state.effective_scheme(SystemAppearance::Dark), scheme);
    }

    #[test]
    fn legacy_palette_typography_migrates_only_in_draft_and_survives_curated_save() {
        let directory = tempfile::tempdir().unwrap();
        let mut stored = Settings::default();
        stored.theme.mode = ThemeMode::Custom;
        stored.theme.preferences.active_preset = Some("Legacy A".into());
        stored.theme.preferences.typography_scale = 1.4;
        stored.theme.named_presets.insert(
            "Legacy A".into(),
            ColorScheme {
                window_fill: rgba(31, 79, 127, 255),
                ..ColorScheme::dark()
            },
        );
        let path = directory.path().join("settings.json");
        let mut legacy = serde_json::to_value(&stored).unwrap();
        legacy["theme"]
            .as_object_mut()
            .unwrap()
            .remove("named_preset_typography_scales");
        let legacy_bytes = serde_json::to_vec_pretty(&legacy).unwrap();
        std::fs::write(&path, &legacy_bytes).unwrap();
        let path = path.to_str().unwrap().to_owned();

        let ctx = egui::Context::default();
        let mut state = ThemeSettingsDialogState::default();
        state.begin_session(&ctx, &path);
        assert!(
            state
                .persisted_baseline
                .as_ref()
                .unwrap()
                .named_preset_typography_scales
                .is_empty(),
            "the raw loaded theme remains the exact conflict baseline"
        );
        assert_eq!(state.draft.named_preset_typography_scale("Legacy A"), 1.4);
        assert_eq!(std::fs::read(&path).unwrap(), legacy_bytes);

        assert!(state.select_curated_preset("obsidian-lime"));
        assert_eq!(state.draft.preferences.typography_scale, 1.0);
        state.save_to_path(&ctx, &path).unwrap();
        let saved = Settings::load(&path).unwrap();
        assert_eq!(saved.theme.named_preset_typography_scales["Legacy A"], 1.4);
        assert_eq!(
            saved.theme.named_preset_typography_scales
                [saved.theme.preferences.active_preset.as_deref().unwrap()],
            1.0
        );

        let mut reopened = ThemeSettingsDialogState::default();
        reopened.begin_session(&ctx, &path);
        assert!(reopened.select_saved_preset("Legacy A"));
        assert_eq!(reopened.draft.preferences.typography_scale, 1.4);
    }

    #[test]
    fn invalid_palette_scales_are_normalized_only_at_effective_and_save_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let mut stored = Settings::default();
        stored.theme.preferences.typography_scale = 2.4;
        stored
            .theme
            .named_preset_typography_scales
            .insert("dark".into(), 0.4);
        let path = settings_path(&directory, &stored);
        let original_bytes = std::fs::read(&path).unwrap();

        let ctx = egui::Context::default();
        let mut state = ThemeSettingsDialogState::default();
        state.begin_session(&ctx, &path);

        assert_eq!(state.draft.preferences.typography_scale, 2.4);
        assert_eq!(state.draft.named_preset_typography_scales["dark"], 0.4);
        assert_eq!(state.draft.preferences.normalized().typography_scale, 2.0);
        assert_eq!(state.draft.named_preset_typography_scale("dark"), 0.75);
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);

        state.dirty = true;
        state.save_to_path(&ctx, &path).unwrap();
        let saved = Settings::load(&path).unwrap();
        assert_eq!(saved.theme.preferences.typography_scale, 2.0);
        assert_eq!(saved.theme.named_preset_typography_scales["dark"], 0.75);
    }

    #[test]
    fn saved_palette_typography_scales_restore_independently_after_reload() {
        let directory = tempfile::tempdir().unwrap();
        let stored = Settings::default();
        let path = settings_path(&directory, &stored);
        let ctx = egui::Context::default();
        let mut state = ThemeSettingsDialogState::default();
        state.begin_session(&ctx, &path);
        state.draft.named_presets.insert(
            "Palette A".into(),
            ColorScheme {
                window_fill: rgba(18, 29, 40, 255),
                ..ColorScheme::dark()
            },
        );
        state
            .draft
            .named_preset_typography_scales
            .insert("Palette A".into(), 0.85);
        state.draft.named_presets.insert(
            "Palette B".into(),
            ColorScheme {
                window_fill: rgba(61, 72, 83, 255),
                ..ColorScheme::dark()
            },
        );
        state
            .draft
            .named_preset_typography_scales
            .insert("Palette B".into(), 1.4);
        state.dirty = true;
        state.save_to_path(&ctx, &path).unwrap();

        let mut reopened = ThemeSettingsDialogState::default();
        reopened.begin_session(&ctx, &path);
        assert!(reopened.select_saved_preset("Palette A"));
        assert_eq!(reopened.draft.preferences.typography_scale, 0.85);
        assert!(reopened.select_saved_preset("Palette B"));
        assert_eq!(reopened.draft.preferences.typography_scale, 1.4);
        assert!(reopened.select_saved_preset("Palette A"));
        assert_eq!(reopened.draft.preferences.typography_scale, 0.85);
    }

    #[test]
    fn fixed_base_mode_does_not_expose_a_stale_saved_palette_update_target() {
        let directory = tempfile::tempdir().unwrap();
        let mut stored = Settings::default();
        stored.theme.mode = ThemeMode::Dark;
        stored.theme.preferences.active_preset = Some("dark".into());
        stored
            .theme
            .named_preset_typography_scales
            .insert("dark".into(), 1.4);
        let path = settings_path(&directory, &stored);
        let ctx = egui::Context::default();
        let mut state = ThemeSettingsDialogState::default();

        state.begin_session(&ctx, &path);
        assert_eq!(state.palette_update_target, None);
        assert_eq!(state.draft.mode, ThemeMode::Dark);

        state.draft.mode = ThemeMode::Custom;
        state.base_mode_changed(ThemeMode::Dark);
        assert_eq!(state.palette_update_target.as_deref(), Some("dark"));
        assert_eq!(state.draft.preferences.typography_scale, 1.4);
    }

    #[test]
    fn save_as_and_update_palette_are_explicit_and_preserve_target_through_edits() {
        let directory = tempfile::tempdir().unwrap();
        let stored = Settings::default();
        let path = settings_path(&directory, &stored);
        let ctx = egui::Context::default();
        let mut state = ThemeSettingsDialogState::default();
        state.begin_session(&ctx, &path);

        assert!(state.save_as_palette("  ", SystemAppearance::Dark).is_err());
        assert!(state.draft.named_presets.contains_key("dark"));
        assert!(
            state
                .save_as_palette("  Studio  ", SystemAppearance::Dark)
                .is_ok()
        );
        assert_eq!(state.palette_update_target.as_deref(), Some("Studio"));
        assert_eq!(state.draft.named_preset_typography_scales["Studio"], 1.0);
        assert!(
            state
                .save_as_palette("Studio", SystemAppearance::Dark)
                .is_err()
        );

        state.save_to_path(&ctx, &path).unwrap();
        let original_scheme = state.draft.named_presets["Studio"].clone();
        assert!(state.set_typography_scale(1.4, SystemAppearance::Dark));
        assert_eq!(state.draft.preferences.active_preset, None);
        assert_eq!(state.palette_update_target.as_deref(), Some("Studio"));
        let mut edited = state.effective_scheme(SystemAppearance::Dark);
        edited.window_fill = rgba(194, 103, 42, 255);
        state.apply_manual_scheme(edited.clone());
        assert_eq!(state.palette_update_target.as_deref(), Some("Studio"));
        state.save_to_path(&ctx, &path).unwrap();
        let ordinarily_saved = Settings::load(&path).unwrap().theme;
        assert_eq!(ordinarily_saved.named_presets["Studio"], original_scheme);
        assert_eq!(
            ordinarily_saved.named_preset_typography_scales["Studio"],
            1.0
        );
        assert_eq!(ordinarily_saved.custom_scheme, edited);
        assert_eq!(ordinarily_saved.preferences.typography_scale, 1.4);

        assert!(state.update_palette(SystemAppearance::Dark).unwrap());
        assert_eq!(state.draft.named_presets["Studio"], edited);
        assert_eq!(state.draft.named_preset_typography_scales["Studio"], 1.4);
        assert_eq!(
            state.draft.preferences.active_preset.as_deref(),
            Some("Studio")
        );
    }

    #[test]
    fn full_theme_reset_preserves_saved_palette_colors_and_typography_scales() {
        let mut state = ThemeSettingsDialogState::default();
        state.draft.named_presets.insert(
            "Studio".into(),
            ColorScheme {
                window_fill: rgba(100, 110, 120, 255),
                ..ColorScheme::dark()
            },
        );
        state
            .draft
            .named_preset_typography_scales
            .insert("Studio".into(), 1.65);
        let scheme = state.draft.named_presets["Studio"].clone();

        reset_draft_preserving_saved_palettes(&mut state);

        assert_eq!(state.draft.named_presets["Studio"], scheme);
        assert_eq!(state.draft.named_preset_typography_scales["Studio"], 1.65);
        assert_eq!(state.draft.preferences.typography_scale, 1.0);
        assert_eq!(state.palette_update_target, None);
        assert!(state.dirty);
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
