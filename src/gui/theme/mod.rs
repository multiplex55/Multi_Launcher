//! Theme resolution and Context-owned application of committed theme settings.

use crate::settings::{
    ColorScheme, ThemeColor, ThemeDensity, ThemeMode, ThemePreferences, ThemeSettings,
};
use eframe::egui::style::WidgetVisuals;
use eframe::egui::{self, Plugin, RawInput, Style, Theme, ThemePreference};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAppearance {
    Dark,
    Light,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedAppearance {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticPalette {
    pub window_surface: ThemeColor,
    pub panel_surface: ThemeColor,
    pub primary_text: ThemeColor,
    pub hyperlink: ThemeColor,
    pub control_fill: ThemeColor,
    pub control_border: ThemeColor,
    pub control_hover_fill: ThemeColor,
    pub control_hover_border: ThemeColor,
    pub control_active_fill: ThemeColor,
    pub control_active_border: ThemeColor,
    pub accent: ThemeColor,
    pub focus: ThemeColor,
    pub selection_fill: ThemeColor,
    pub selection_border: ThemeColor,
    pub warning: ThemeColor,
    pub error: ThemeColor,
    pub success: ThemeColor,
}

/// Structural identity of every resolved palette token. This remains stable
/// across map ordering and does not depend on a process-randomized hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteIdentity(pub SemanticPalette);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeMetrics {
    pub body_text_size: f32,
    pub heading_text_size: f32,
    pub small_text_size: f32,
    pub line_height: f32,
    pub item_spacing_x: f32,
    pub item_spacing_y: f32,
    pub section_spacing: f32,
    pub window_padding: f32,
    pub panel_padding: f32,
    pub button_padding_x: f32,
    pub button_padding_y: f32,
    pub corner_radius: f32,
    pub stroke_width: f32,
    pub shadow_blur: f32,
    pub icon_size: f32,
    pub row_min_height: f32,
}

/// Exact bit-pattern identity of the resolved metric values. Query/list
/// scaling is intentionally owned elsewhere and is not part of this identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsIdentity(pub [u32; 16]);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedTheme {
    pub appearance: ResolvedAppearance,
    pub palette: SemanticPalette,
    pub metrics: ThemeMetrics,
    pub palette_identity: PaletteIdentity,
    pub metrics_identity: MetricsIdentity,
}

pub fn resolve_theme(
    settings: &ThemeSettings,
    system_appearance: SystemAppearance,
) -> ResolvedTheme {
    let appearance = resolved_appearance(settings.mode, system_appearance);
    let scheme = effective_color_scheme(settings, system_appearance);

    let mut palette = palette_from_scheme(&scheme);
    if let Some(accent) = settings.preferences.accent {
        palette.accent = accent;
        palette.focus = accent;
    }
    let metrics = metrics_from_preferences(&settings.preferences.normalized());
    let palette_identity = PaletteIdentity(palette);
    let metrics_identity = metrics.identity();
    ResolvedTheme {
        appearance,
        palette,
        metrics,
        palette_identity,
        metrics_identity,
    }
}

pub(crate) fn effective_color_scheme(
    settings: &ThemeSettings,
    system_appearance: SystemAppearance,
) -> ColorScheme {
    match settings.mode {
        ThemeMode::System => match system_appearance {
            SystemAppearance::Light => ColorScheme::light(),
            SystemAppearance::Dark | SystemAppearance::Unknown => ColorScheme::dark(),
        },
        ThemeMode::Dark => ColorScheme::dark(),
        ThemeMode::Light => ColorScheme::light(),
        ThemeMode::Custom => settings
            .preferences
            .active_preset
            .as_deref()
            .and_then(|name| settings.named_presets.get(name))
            .unwrap_or(&settings.custom_scheme)
            .clone(),
    }
}

fn resolved_appearance(mode: ThemeMode, system_appearance: SystemAppearance) -> ResolvedAppearance {
    match mode {
        ThemeMode::System => match system_appearance {
            SystemAppearance::Light => ResolvedAppearance::Light,
            SystemAppearance::Dark | SystemAppearance::Unknown => ResolvedAppearance::Dark,
        },
        ThemeMode::Dark | ThemeMode::Custom => ResolvedAppearance::Dark,
        ThemeMode::Light => ResolvedAppearance::Light,
    }
}

fn palette_from_scheme(scheme: &ColorScheme) -> SemanticPalette {
    SemanticPalette {
        window_surface: scheme.window_fill,
        panel_surface: scheme.panel_fill,
        primary_text: scheme.text,
        hyperlink: scheme.hyperlink,
        control_fill: scheme.widget_inactive_fill,
        control_border: scheme.widget_inactive_stroke,
        control_hover_fill: scheme.widget_hovered_fill,
        control_hover_border: scheme.widget_hovered_stroke,
        control_active_fill: scheme.widget_active_fill,
        control_active_border: scheme.widget_active_stroke,
        accent: scheme.widget_active_fill,
        focus: scheme.widget_active_stroke,
        selection_fill: scheme.selection_bg,
        selection_border: scheme.selection_stroke,
        warning: scheme.warn_accent,
        error: scheme.error_accent,
        success: scheme.success_accent,
    }
}

fn metrics_from_preferences(preferences: &ThemePreferences) -> ThemeMetrics {
    let scale = preferences.typography_scale;
    let (density, radius, shadow_blur) = match preferences.density {
        ThemeDensity::Compact => (1.0, 4.0, 8.0),
        ThemeDensity::Comfortable => (1.25, 7.0, 12.0),
    };
    let body_text_size = 14.0 * scale;
    let heading_text_size = 18.0 * scale;
    let small_text_size = 12.0 * scale;
    let line_height = 18.0 * scale;
    ThemeMetrics {
        body_text_size,
        heading_text_size,
        small_text_size,
        line_height,
        item_spacing_x: 6.0 * density,
        item_spacing_y: 4.0 * density,
        section_spacing: 8.0 * density,
        window_padding: 8.0 * density,
        panel_padding: 6.0 * density,
        button_padding_x: 8.0 * density,
        button_padding_y: 4.0 * density,
        corner_radius: radius,
        stroke_width: 1.0,
        shadow_blur,
        icon_size: 16.0 * scale,
        row_min_height: 28.0 * density * scale,
    }
}

impl ThemeMetrics {
    fn identity(self) -> MetricsIdentity {
        MetricsIdentity([
            self.body_text_size.to_bits(),
            self.heading_text_size.to_bits(),
            self.small_text_size.to_bits(),
            self.line_height.to_bits(),
            self.item_spacing_x.to_bits(),
            self.item_spacing_y.to_bits(),
            self.section_spacing.to_bits(),
            self.window_padding.to_bits(),
            self.panel_padding.to_bits(),
            self.button_padding_x.to_bits(),
            self.button_padding_y.to_bits(),
            self.corner_radius.to_bits(),
            self.stroke_width.to_bits(),
            self.shadow_blur.to_bits(),
            self.icon_size.to_bits(),
            self.row_min_height.to_bits(),
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AppliedStyleIdentity {
    appearance: ResolvedAppearance,
    palette: PaletteIdentity,
    metrics: MetricsIdentity,
}

#[derive(Clone, Copy)]
struct AppliedThemeSlot {
    resolved: ResolvedTheme,
    identity: AppliedStyleIdentity,
}

/// The one owner of committed theme preferences and the two egui style slots
/// associated with a shared Context. Styles are always rebuilt from the
/// captured baseline, so repeated publications never compound scaling.
struct ThemeRuntimePlugin {
    committed: ThemeSettings,
    preview: Option<ThemeSettings>,
    dark_baseline: Arc<Style>,
    light_baseline: Arc<Style>,
    dark: Option<AppliedThemeSlot>,
    light: Option<AppliedThemeSlot>,
    theme_preference: Option<ThemePreference>,
    known_viewports: Vec<egui::ViewportId>,
}

impl ThemeRuntimePlugin {
    fn new(ctx: &egui::Context, committed: &ThemeSettings) -> Self {
        let mut known_viewports = vec![egui::ViewportId::ROOT, ctx.viewport_id()];
        known_viewports.sort_unstable();
        known_viewports.dedup();
        Self {
            committed: committed.clone(),
            preview: None,
            dark_baseline: ctx.style_of(Theme::Dark),
            light_baseline: ctx.style_of(Theme::Light),
            dark: None,
            light: None,
            theme_preference: None,
            known_viewports,
        }
    }

    fn publish_committed(&mut self, ctx: &egui::Context, committed: &ThemeSettings) -> bool {
        // Preserve inactive or unknown preference values even when they do not
        // change either resolved style slot.
        self.committed = committed.clone();
        self.apply_effective(ctx)
    }

    fn publish_preview(&mut self, ctx: &egui::Context, preview: &ThemeSettings) -> bool {
        self.preview = Some(preview.clone());
        self.apply_effective(ctx)
    }

    fn clear_preview(&mut self, ctx: &egui::Context) -> bool {
        if self.preview.take().is_none() {
            return false;
        }
        self.apply_effective(ctx)
    }

    fn apply_effective(&mut self, ctx: &egui::Context) -> bool {
        let effective = self.preview.as_ref().unwrap_or(&self.committed);
        let preference = theme_preference(effective.mode);
        let mut changed = self.theme_preference != Some(preference);
        if changed {
            ctx.set_theme(preference);
            self.theme_preference = Some(preference);
        }

        let dark = resolve_theme(effective, SystemAppearance::Dark);
        let light = resolve_theme(effective, SystemAppearance::Light);
        changed |= self.apply_slot(ctx, Theme::Dark, dark);
        changed |= self.apply_slot(ctx, Theme::Light, light);
        changed
    }

    fn apply_slot(&mut self, ctx: &egui::Context, slot: Theme, resolved: ResolvedTheme) -> bool {
        let identity = AppliedStyleIdentity {
            appearance: resolved.appearance,
            palette: resolved.palette_identity,
            metrics: resolved.metrics_identity,
        };
        let previous = match slot {
            Theme::Dark => self.dark,
            Theme::Light => self.light,
        };
        if previous.is_some_and(|previous| previous.identity == identity) {
            return false;
        }

        let baseline = match slot {
            Theme::Dark => &self.dark_baseline,
            Theme::Light => &self.light_baseline,
        };
        let mut style = (**baseline).clone();
        apply_resolved_theme(&mut style, resolved);
        ctx.set_style_of(slot, Arc::new(style));
        let applied = Some(AppliedThemeSlot { resolved, identity });
        match slot {
            Theme::Dark => self.dark = applied,
            Theme::Light => self.light = applied,
        }
        true
    }

    fn resolved_for(&self, theme: Theme) -> Option<ResolvedTheme> {
        match theme {
            Theme::Dark => self.dark,
            Theme::Light => self.light,
        }
        .map(|slot| slot.resolved)
    }
}

impl Plugin for ThemeRuntimePlugin {
    fn debug_name(&self) -> &'static str {
        "Multi Launcher theme runtime"
    }

    fn input_hook(&mut self, _ctx: &egui::Context, input: &mut RawInput) {
        self.known_viewports.clear();
        self.known_viewports.extend(input.viewports.keys().copied());
        if !self.known_viewports.contains(&input.viewport_id) {
            self.known_viewports.push(input.viewport_id);
        }
        if !self.known_viewports.contains(&egui::ViewportId::ROOT) {
            self.known_viewports.push(egui::ViewportId::ROOT);
        }
        self.known_viewports.sort_unstable();
        self.known_viewports.dedup();
    }
}

fn theme_preference(mode: ThemeMode) -> ThemePreference {
    match mode {
        ThemeMode::System => ThemePreference::System,
        ThemeMode::Dark | ThemeMode::Custom => ThemePreference::Dark,
        ThemeMode::Light => ThemePreference::Light,
    }
}

fn ensure_runtime(ctx: &egui::Context, initial: &ThemeSettings) {
    if ctx.with_plugin::<ThemeRuntimePlugin, _>(|_| ()).is_none() {
        ctx.add_plugin(ThemeRuntimePlugin::new(ctx, initial));
    }
}

/// Install and apply the committed theme before the first egui pass.
pub(crate) fn initialize_runtime(ctx: &egui::Context, committed: &ThemeSettings) {
    ensure_runtime(ctx, committed);
    ctx.with_plugin::<ThemeRuntimePlugin, _>(|runtime| {
        runtime.publish_committed(ctx, committed);
    });
}

/// Publish only a successfully committed theme. A no-op publication preserves
/// both style Arcs and does not request another frame.
pub(crate) fn publish_committed(ctx: &egui::Context, committed: &ThemeSettings) -> bool {
    ensure_runtime(ctx, committed);
    let Some((changed, viewports)) = ctx.with_plugin::<ThemeRuntimePlugin, _>(|runtime| {
        let changed = runtime.publish_committed(ctx, committed);
        let viewports = changed.then(|| runtime.known_viewports.clone());
        (changed, viewports)
    }) else {
        return false;
    };
    if let Some(viewports) = viewports {
        for viewport_id in viewports {
            ctx.request_repaint_of(viewport_id);
        }
    }
    changed
}

/// Apply a draft over the latest committed settings without mutating either.
pub(crate) fn publish_preview(ctx: &egui::Context, preview: &ThemeSettings) -> bool {
    let Some((changed, viewports)) = ctx.with_plugin::<ThemeRuntimePlugin, _>(|runtime| {
        let changed = runtime.publish_preview(ctx, preview);
        let viewports = changed.then(|| runtime.known_viewports.clone());
        (changed, viewports)
    }) else {
        return false;
    };
    if let Some(viewports) = viewports {
        for viewport_id in viewports {
            ctx.request_repaint_of(viewport_id);
        }
    }
    changed
}

/// Remove the temporary draft overlay and reveal the latest committed theme.
pub(crate) fn clear_preview(ctx: &egui::Context) -> bool {
    let Some((changed, viewports)) = ctx.with_plugin::<ThemeRuntimePlugin, _>(|runtime| {
        let changed = runtime.clear_preview(ctx);
        let viewports = changed.then(|| runtime.known_viewports.clone());
        (changed, viewports)
    }) else {
        return false;
    };
    if let Some(viewports) = viewports {
        for viewport_id in viewports {
            ctx.request_repaint_of(viewport_id);
        }
    }
    changed
}

/// Return the semantic theme whose style slot is active in the Context.
/// Consumers use this rather than independently resolving stored settings.
pub(crate) fn resolved_for_current_context(ctx: &egui::Context) -> Option<ResolvedTheme> {
    ctx.with_plugin::<ThemeRuntimePlugin, _>(|runtime| runtime.resolved_for(ctx.theme()))
        .flatten()
}

fn apply_resolved_theme(style: &mut Style, resolved: ResolvedTheme) {
    let metrics = resolved.metrics;
    let palette = resolved.palette;
    let visuals = &mut style.visuals;
    let text = to_color32(palette.primary_text);
    let window = to_color32(palette.window_surface);
    let panel = to_color32(palette.panel_surface);
    let width = metrics.stroke_width;
    let rounding = egui::CornerRadius::from(metrics.corner_radius);

    visuals.dark_mode = resolved.appearance == ResolvedAppearance::Dark;
    visuals.window_fill = window;
    visuals.panel_fill = panel;
    visuals.extreme_bg_color = window;
    visuals.text_edit_bg_color = Some(window);
    visuals.code_bg_color = panel;
    visuals.faint_bg_color = panel;
    visuals.hyperlink_color = to_color32(palette.hyperlink);
    visuals.warn_fg_color = to_color32(palette.warning);
    visuals.error_fg_color = to_color32(palette.error);
    visuals.window_stroke = egui::Stroke::new(width, to_color32(palette.control_border));
    visuals.window_corner_radius = rounding;
    visuals.menu_corner_radius = rounding;
    visuals.window_shadow.blur = metrics.shadow_blur.round().clamp(0.0, u8::MAX as f32) as u8;
    visuals.popup_shadow.blur = metrics.shadow_blur.round().clamp(0.0, u8::MAX as f32) as u8;
    visuals.selection.bg_fill = to_color32(palette.selection_fill);
    visuals.selection.stroke = egui::Stroke::new(width, to_color32(palette.selection_border));
    visuals.text_cursor.stroke = egui::Stroke::new(width, to_color32(palette.accent));

    let widgets = &mut visuals.widgets;
    apply_widget_visuals(
        &mut widgets.noninteractive,
        panel,
        to_color32(palette.control_border),
        text,
        rounding,
        width,
    );
    apply_widget_visuals(
        &mut widgets.inactive,
        to_color32(palette.control_fill),
        to_color32(palette.control_border),
        text,
        rounding,
        width,
    );
    apply_widget_visuals(
        &mut widgets.hovered,
        to_color32(palette.control_hover_fill),
        to_color32(palette.control_hover_border),
        text,
        rounding,
        width,
    );
    apply_widget_visuals(
        &mut widgets.active,
        to_color32(palette.control_active_fill),
        to_color32(palette.focus),
        text,
        rounding,
        width,
    );
    apply_widget_visuals(
        &mut widgets.open,
        to_color32(palette.control_active_fill),
        to_color32(palette.control_active_border),
        text,
        rounding,
        width,
    );

    style.spacing.item_spacing = egui::vec2(metrics.item_spacing_x, metrics.item_spacing_y);
    style.spacing.window_margin = egui::Margin::same(metrics.window_padding.round() as i8);
    style.spacing.menu_margin = egui::Margin::same(metrics.panel_padding.round() as i8);
    style.spacing.scroll.content_margin = egui::Margin::same(metrics.panel_padding.round() as i8);
    style.spacing.button_padding = egui::vec2(metrics.button_padding_x, metrics.button_padding_y);
    style.spacing.interact_size.y = metrics.row_min_height;
    style.spacing.extra_text_line_spacing = (metrics.line_height - metrics.body_text_size).max(0.0);
    style.spacing.menu_spacing = metrics.section_spacing;
    style.spacing.icon_width = metrics.icon_size;
    style.spacing.icon_width_inner = metrics.icon_size * 0.75;

    for (text_style, font_id) in &mut style.text_styles {
        font_id.size = match text_style {
            egui::TextStyle::Heading => metrics.heading_text_size,
            egui::TextStyle::Small => metrics.small_text_size,
            egui::TextStyle::Body | egui::TextStyle::Button | egui::TextStyle::Monospace => {
                metrics.body_text_size
            }
            egui::TextStyle::Name(_) => font_id.size,
        };
    }
}

fn apply_widget_visuals(
    visuals: &mut WidgetVisuals,
    fill: egui::Color32,
    border: egui::Color32,
    text: egui::Color32,
    rounding: egui::CornerRadius,
    stroke_width: f32,
) {
    visuals.bg_fill = fill;
    visuals.weak_bg_fill = fill;
    visuals.bg_stroke = egui::Stroke::new(stroke_width, border);
    visuals.corner_radius = rounding;
    visuals.fg_stroke = egui::Stroke::new(stroke_width, text);
}

fn to_color32(color: ThemeColor) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

#[cfg(test)]
mod tests {
    use super::{
        ResolvedAppearance, SystemAppearance, clear_preview, initialize_runtime, publish_committed,
        publish_preview, resolve_theme, resolved_for_current_context, to_color32,
    };
    use crate::settings::{
        ColorScheme, ThemeColor, ThemeDensity, ThemeMode, ThemePreferences, ThemeSettings,
    };
    use eframe::egui::{self, RawInput, Theme};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn first_ui_window_fill(ctx: &egui::Context, input: RawInput) -> egui::Color32 {
        let mut fill = None;
        let output = ctx.run_ui(input, |ui| fill = Some(ui.visuals().window_fill));
        output.drop_without_applying_deltas();
        fill.expect("run_ui invokes the root UI closure")
    }

    #[test]
    fn startup_applies_both_system_slots_before_first_root_ui_and_fixed_modes_win() {
        let dark = egui::Color32::from_rgb(
            ColorScheme::dark().window_fill.r,
            ColorScheme::dark().window_fill.g,
            ColorScheme::dark().window_fill.b,
        );
        let light = egui::Color32::from_rgb(
            ColorScheme::light().window_fill.r,
            ColorScheme::light().window_fill.g,
            ColorScheme::light().window_fill.b,
        );

        let ctx = egui::Context::default();
        initialize_runtime(&ctx, &ThemeSettings::default());
        assert_eq!(ctx.style_of(Theme::Dark).visuals.window_fill, dark);
        assert_eq!(ctx.style_of(Theme::Light).visuals.window_fill, light);
        assert_eq!(
            first_ui_window_fill(
                &ctx,
                RawInput {
                    system_theme: Some(Theme::Light),
                    ..Default::default()
                },
            ),
            light,
            "the first root UI follows the supplied system theme"
        );
        assert_eq!(
            resolved_for_current_context(&ctx).unwrap().appearance,
            ResolvedAppearance::Light
        );
        assert_eq!(
            first_ui_window_fill(
                &ctx,
                RawInput {
                    system_theme: Some(Theme::Dark),
                    ..Default::default()
                },
            ),
            dark,
            "System switches between already-prepared slots without a runtime poll"
        );
        assert_eq!(
            first_ui_window_fill(&ctx, RawInput::default()),
            dark,
            "an unknown OS appearance uses egui's dark fallback"
        );

        for (mode, system_theme, expected_theme, expected_fill) in [
            (ThemeMode::Dark, Theme::Light, Theme::Dark, dark),
            (ThemeMode::Light, Theme::Dark, Theme::Light, light),
        ] {
            let fixed_ctx = egui::Context::default();
            let settings = ThemeSettings {
                mode,
                ..ThemeSettings::default()
            };
            initialize_runtime(&fixed_ctx, &settings);
            assert_eq!(
                first_ui_window_fill(
                    &fixed_ctx,
                    RawInput {
                        system_theme: Some(system_theme),
                        ..Default::default()
                    },
                ),
                expected_fill
            );
            assert_eq!(fixed_ctx.theme(), expected_theme);
        }

        let custom_ctx = egui::Context::default();
        let custom_color = ThemeColor {
            r: 11,
            g: 29,
            b: 47,
            a: 255,
        };
        let custom = ThemeSettings {
            mode: ThemeMode::Custom,
            custom_scheme: ColorScheme {
                window_fill: custom_color,
                ..ColorScheme::dark()
            },
            ..ThemeSettings::default()
        };
        initialize_runtime(&custom_ctx, &custom);
        assert_eq!(custom_ctx.theme(), Theme::Dark);
        let custom_fill = egui::Color32::from_rgb(custom_color.r, custom_color.g, custom_color.b);
        assert_eq!(
            custom_ctx.style_of(Theme::Dark).visuals.window_fill,
            custom_fill
        );
        assert_eq!(
            custom_ctx.style_of(Theme::Light).visuals.window_fill,
            custom_fill
        );
    }

    #[test]
    fn publication_retains_noop_slots_and_rebuilds_palette_and_metrics_from_baselines() {
        let ctx = egui::Context::default();
        let mut settings = ThemeSettings {
            mode: ThemeMode::Custom,
            ..ThemeSettings::default()
        };
        settings.preferences.active_preset = Some("active".into());
        settings
            .named_presets
            .insert("active".into(), ColorScheme::dark());
        settings
            .named_presets
            .insert("inactive".into(), ColorScheme::light());
        initialize_runtime(&ctx, &settings);

        // Consume egui's initial outstanding repaint before testing idle no-ops.
        for _ in 0..4 {
            if !ctx.has_requested_repaint_for(&egui::ViewportId::ROOT) {
                break;
            }
            ctx.run_ui(RawInput::default(), |_| {})
                .drop_without_applying_deltas();
        }
        assert!(!ctx.has_requested_repaint_for(&egui::ViewportId::ROOT));
        let initial_dark = ctx.style_of(Theme::Dark);
        let initial_light = ctx.style_of(Theme::Light);
        let initial_resolved = resolved_for_current_context(&ctx).unwrap();
        let mut inactive_edit = settings.clone();
        inactive_edit
            .named_presets
            .insert("inactive".into(), ColorScheme::dark());
        assert!(!publish_committed(&ctx, &inactive_edit));
        assert!(Arc::ptr_eq(&initial_dark, &ctx.style_of(Theme::Dark)));
        assert!(Arc::ptr_eq(&initial_light, &ctx.style_of(Theme::Light)));
        assert!(!ctx.has_requested_repaint_for(&egui::ViewportId::ROOT));

        assert!(!publish_preview(&ctx, &inactive_edit));
        assert!(Arc::ptr_eq(&initial_dark, &ctx.style_of(Theme::Dark)));
        assert!(Arc::ptr_eq(&initial_light, &ctx.style_of(Theme::Light)));
        assert!(!ctx.has_requested_repaint_for(&egui::ViewportId::ROOT));
        assert!(!clear_preview(&ctx));
        assert!(Arc::ptr_eq(&initial_dark, &ctx.style_of(Theme::Dark)));
        assert!(!ctx.has_requested_repaint_for(&egui::ViewportId::ROOT));

        let mut palette_edit = inactive_edit.clone();
        palette_edit.named_presets.get_mut("active").unwrap().text = ThemeColor {
            r: 9,
            g: 81,
            b: 173,
            a: 255,
        };
        assert!(publish_committed(&ctx, &palette_edit));
        let palette_resolved = resolved_for_current_context(&ctx).unwrap();
        assert_ne!(
            initial_resolved.palette_identity,
            palette_resolved.palette_identity
        );
        assert_eq!(
            initial_resolved.metrics_identity,
            palette_resolved.metrics_identity
        );
        let palette_style = ctx.style_of(Theme::Dark);
        assert!(!Arc::ptr_eq(&initial_dark, &palette_style));
        assert!(!publish_committed(&ctx, &palette_edit));
        assert!(Arc::ptr_eq(&palette_style, &ctx.style_of(Theme::Dark)));

        let mut metric_edit = palette_edit.clone();
        metric_edit.preferences.density = ThemeDensity::Comfortable;
        metric_edit.preferences.typography_scale = 1.5;
        assert!(publish_committed(&ctx, &metric_edit));
        let metric_resolved = resolved_for_current_context(&ctx).unwrap();
        assert_eq!(
            palette_resolved.palette_identity,
            metric_resolved.palette_identity
        );
        assert_ne!(
            palette_resolved.metrics_identity,
            metric_resolved.metrics_identity
        );
        let metric_style = ctx.style_of(Theme::Dark);
        assert_eq!(metric_style.text_styles[&egui::TextStyle::Body].size, 21.0);
        assert_eq!(
            metric_style.text_styles[&egui::TextStyle::Heading].size,
            27.0
        );
        assert_eq!(metric_style.spacing.item_spacing, egui::vec2(7.5, 5.0));
        assert!(!publish_committed(&ctx, &metric_edit));
        assert!(Arc::ptr_eq(&metric_style, &ctx.style_of(Theme::Dark)));
    }

    #[test]
    fn one_context_runtime_repaints_known_secondary_viewports_without_opening_them() {
        let ctx = egui::Context::default();
        let initial = ThemeSettings::default();
        initialize_runtime(&ctx, &initial);
        let secondary = egui::ViewportId::from_hash_of("theme-runtime-secondary");
        let mut viewports = egui::ViewportIdMap::default();
        viewports.insert(egui::ViewportId::ROOT, Default::default());
        viewports.insert(
            secondary,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                title: Some("secondary".into()),
                ..Default::default()
            },
        );
        let output = ctx.run_ui(
            RawInput {
                viewport_id: secondary,
                viewports,
                ..Default::default()
            },
            |ui| assert_eq!(ui.ctx().viewport_id(), secondary),
        );
        output.drop_without_applying_deltas();

        let committed = ThemeSettings {
            mode: ThemeMode::Light,
            ..initial
        };
        assert!(publish_committed(&ctx, &committed));
        assert!(ctx.has_requested_repaint_for(&egui::ViewportId::ROOT));
        assert!(ctx.has_requested_repaint_for(&secondary));
        assert_eq!(ctx.theme(), Theme::Light);

        let mut viewports = egui::ViewportIdMap::default();
        viewports.insert(egui::ViewportId::ROOT, Default::default());
        viewports.insert(
            secondary,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                title: Some("secondary".into()),
                ..Default::default()
            },
        );
        let mut child_fill = None;
        let output = ctx.run_ui(
            RawInput {
                viewport_id: secondary,
                viewports,
                ..Default::default()
            },
            |ui| child_fill = Some(ui.visuals().window_fill),
        );
        output.drop_without_applying_deltas();
        assert_eq!(
            child_fill,
            Some(to_color32(ColorScheme::light().window_fill)),
            "the child viewport uses the single Context's committed style without a root pass"
        );
    }

    fn settings_for(mode: ThemeMode) -> ThemeSettings {
        ThemeSettings {
            mode,
            ..ThemeSettings::default()
        }
    }

    #[test]
    fn mode_authority_and_custom_preset_fallbacks_are_explicit() {
        let mut settings = settings_for(ThemeMode::Dark);
        settings
            .named_presets
            .insert("chosen".into(), ColorScheme::light());
        settings.preferences.active_preset = Some("chosen".into());

        let dark = resolve_theme(&settings, SystemAppearance::Light);
        assert_eq!(dark.appearance, ResolvedAppearance::Dark);
        assert_eq!(dark.palette.window_surface, ColorScheme::dark().window_fill);

        settings.mode = ThemeMode::Light;
        let light = resolve_theme(&settings, SystemAppearance::Dark);
        assert_eq!(light.appearance, ResolvedAppearance::Light);
        assert_eq!(
            light.palette.window_surface,
            ColorScheme::light().window_fill
        );

        settings.mode = ThemeMode::System;
        let system_light = resolve_theme(&settings, SystemAppearance::Light);
        let system_dark = resolve_theme(&settings, SystemAppearance::Dark);
        let system_unknown = resolve_theme(&settings, SystemAppearance::Unknown);
        assert_eq!(system_light.palette, light.palette);
        assert_eq!(system_dark.palette, dark.palette);
        assert_eq!(system_unknown.palette, dark.palette);

        settings.mode = ThemeMode::Custom;
        let custom_preset = resolve_theme(&settings, SystemAppearance::Light);
        assert_eq!(custom_preset.palette, light.palette);

        let custom_fallback = ColorScheme {
            text: ThemeColor {
                r: 1,
                g: 2,
                b: 3,
                a: 41,
            },
            ..ColorScheme::dark()
        };
        settings.custom_scheme = custom_fallback.clone();
        settings.preferences.active_preset = Some("missing-preset".into());
        let fallback = resolve_theme(&settings, SystemAppearance::Light);
        assert_eq!(fallback.palette.primary_text, custom_fallback.text);
        assert_eq!(
            settings.preferences.active_preset.as_deref(),
            Some("missing-preset")
        );
        assert_eq!(settings.custom_scheme, custom_fallback);

        settings.preferences.active_preset = None;
        assert_eq!(
            resolve_theme(&settings, SystemAppearance::Unknown)
                .palette
                .primary_text,
            custom_fallback.text
        );
    }

    #[test]
    fn optional_accent_changes_only_accent_and_focus_tokens() {
        let mut settings = settings_for(ThemeMode::Custom);
        let scheme = ColorScheme::dark();
        settings.custom_scheme = scheme.clone();
        let accent = ThemeColor {
            r: 12,
            g: 34,
            b: 56,
            a: 78,
        };
        settings.preferences.accent = Some(accent);

        let resolved = resolve_theme(&settings, SystemAppearance::Dark);
        assert_eq!(resolved.palette.accent, accent);
        assert_eq!(resolved.palette.focus, accent);
        assert_eq!(resolved.palette.hyperlink, scheme.hyperlink);
        assert_eq!(resolved.palette.selection_fill, scheme.selection_bg);
        assert_eq!(resolved.palette.selection_border, scheme.selection_stroke);
        assert_eq!(settings.custom_scheme, scheme);
    }

    #[test]
    fn identities_follow_resolved_values_and_ignore_inactive_map_order() {
        let active = ColorScheme::light();
        let mut first = settings_for(ThemeMode::Custom);
        first.preferences.active_preset = Some("active".into());
        first.named_presets = HashMap::from([
            ("active".into(), active.clone()),
            ("unused-a".into(), ColorScheme::dark()),
            ("unused-b".into(), ColorScheme::dark()),
        ]);
        let mut second = settings_for(ThemeMode::Custom);
        second.preferences.active_preset = Some("active".into());
        second.named_presets = HashMap::from([
            ("unused-b".into(), ColorScheme::light()),
            ("active".into(), active),
            ("unused-a".into(), ColorScheme::light()),
        ]);

        let base = resolve_theme(&first, SystemAppearance::Unknown);
        let reordered = resolve_theme(&second, SystemAppearance::Light);
        assert_eq!(base.palette, reordered.palette);
        assert_eq!(base.palette_identity, reordered.palette_identity);
        assert_eq!(base.metrics_identity, reordered.metrics_identity);

        let mut palette_only = first.clone();
        palette_only.named_presets.get_mut("active").unwrap().text = ThemeColor {
            r: 1,
            g: 2,
            b: 3,
            a: 255,
        };
        let palette_change = resolve_theme(&palette_only, SystemAppearance::Unknown);
        assert_ne!(base.palette_identity, palette_change.palette_identity);
        assert_eq!(base.metrics_identity, palette_change.metrics_identity);

        let mut density_change = first.clone();
        density_change.preferences.density = ThemeDensity::Comfortable;
        let denser = resolve_theme(&density_change, SystemAppearance::Unknown);
        assert_eq!(base.palette_identity, denser.palette_identity);
        assert_ne!(base.metrics_identity, denser.metrics_identity);

        let mut typography_change = first;
        typography_change.preferences.typography_scale = 1.5;
        let larger = resolve_theme(&typography_change, SystemAppearance::Unknown);
        assert_eq!(base.palette_identity, larger.palette_identity);
        assert_ne!(base.metrics_identity, larger.metrics_identity);
    }

    #[test]
    fn invalid_metric_scale_uses_compact_default_without_rewriting_preferences() {
        let mut settings = ThemeSettings::default();
        settings.preferences.typography_scale = f32::NAN;
        settings.preferences.active_preset = Some("unknown-and-retained".into());
        let resolved = resolve_theme(&settings, SystemAppearance::Unknown);
        let default = resolve_theme(&ThemeSettings::default(), SystemAppearance::Unknown);

        assert!(settings.preferences.typography_scale.is_nan());
        assert_eq!(
            settings.preferences.active_preset.as_deref(),
            Some("unknown-and-retained")
        );
        assert_eq!(resolved.metrics, default.metrics);
        assert_eq!(resolved.metrics_identity, default.metrics_identity);

        settings.preferences.typography_scale = -100.0;
        assert_eq!(
            resolve_theme(&settings, SystemAppearance::Unknown)
                .metrics
                .body_text_size,
            14.0 * ThemePreferences::MIN_TYPOGRAPHY_SCALE
        );
        settings.preferences.typography_scale = f32::INFINITY;
        assert_eq!(
            resolve_theme(&settings, SystemAppearance::Unknown).metrics,
            default.metrics
        );
        settings.preferences.typography_scale = 100.0;
        assert_eq!(
            resolve_theme(&settings, SystemAppearance::Unknown)
                .metrics
                .body_text_size,
            28.0
        );
    }
}
