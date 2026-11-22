use gpui_kit::component::theme::{
    Theme as KitTheme, ThemeColor as KitThemeColor, ThemeMode as KitThemeMode,
};
use gpui_kit::{App, Global, Hsla, Rgba, WindowAppearance, px};
use multi_launcher::{
    gui::theme::{
        ResolvedAppearance, ResolvedTheme, SemanticPalette, SystemAppearance, resolve_theme,
    },
    settings::{ThemeColor, ThemeMode, ThemeSettings},
};

#[derive(Clone, Copy)]
pub(crate) struct AppAppearance {
    pub resolved: ResolvedTheme,
}

impl Global for AppAppearance {}

pub(crate) fn should_observe_system(settings: &ThemeSettings) -> bool {
    settings.mode == ThemeMode::System
}

pub(crate) fn system_appearance(appearance: WindowAppearance) -> SystemAppearance {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => SystemAppearance::Dark,
        WindowAppearance::Light | WindowAppearance::VibrantLight => SystemAppearance::Light,
    }
}

fn kit_mode(appearance: ResolvedAppearance) -> KitThemeMode {
    match appearance {
        ResolvedAppearance::Light => KitThemeMode::Light,
        ResolvedAppearance::Dark => KitThemeMode::Dark,
    }
}

pub(crate) fn current(cx: &App) -> ResolvedTheme {
    cx.global::<AppAppearance>().resolved
}

pub(crate) fn apply_saved_theme(
    settings: &ThemeSettings,
    appearance: SystemAppearance,
    cx: &mut App,
) -> ResolvedTheme {
    let resolved = resolve_theme(settings, appearance);

    // Kit reloads its registered mode palette when changing modes, so custom
    // semantic colors and metrics must be applied in a separate update.
    KitTheme::change(kit_mode(resolved.appearance), None, cx);
    KitTheme::sync_scrollbar_appearance(cx);
    KitTheme::update(cx, |theme| apply_theme_values(theme, resolved));
    cx.set_global(AppAppearance { resolved });
    resolved
}

fn apply_theme_values(theme: &mut KitTheme, resolved: ResolvedTheme) {
    apply_palette(&mut theme.colors, resolved.palette);

    let scale = resolved.metrics.body_text_size / 14.0;
    theme.font_size = px(16.0 * scale);
    theme.mono_font_size = px(13.0 * scale);
    theme.radius = px(resolved.metrics.corner_radius);
    theme.radius_lg = px(resolved.metrics.corner_radius);
}

fn apply_palette(colors: &mut KitThemeColor, palette: SemanticPalette) {
    let window = to_hsla(palette.window_surface);
    let panel = to_hsla(palette.panel_surface);
    let text = to_hsla(palette.primary_text);
    let control_fill = to_hsla(palette.control_fill);
    let control_border = to_hsla(palette.control_border);
    let hover_fill = to_hsla(palette.control_hover_fill);
    let active_fill = to_hsla(palette.control_active_fill);
    let active_border = to_hsla(palette.control_active_border);
    let accent = to_hsla(palette.accent);
    let focus = to_hsla(palette.focus);
    let selection = to_hsla(palette.selection_fill);
    let selection_border = to_hsla(palette.selection_border);
    let link = to_hsla(palette.hyperlink);
    let warning = to_hsla(palette.warning);
    let error = to_hsla(palette.error);
    let success = to_hsla(palette.success);

    colors.background = window;
    colors.foreground = text;
    colors.border = control_border;
    colors.window_border = control_border;

    colors.sidebar = panel;
    colors.sidebar_foreground = text;
    colors.sidebar_border = control_border;
    colors.list = panel;
    colors.list_even = panel;
    colors.list_head = panel;
    colors.list_hover = hover_fill;
    colors.list_active = selection;
    colors.list_active_border = selection_border;
    colors.popover = panel;
    colors.popover_foreground = text;
    colors.group_box = panel;
    colors.group_box_foreground = text;
    colors.accordion = panel;
    colors.table = panel;
    colors.table_head = panel;
    colors.table_head_foreground = text;
    colors.table_foot = panel;
    colors.table_foot_foreground = text;
    colors.table_even = panel;
    colors.table_hover = hover_fill;
    colors.table_active = selection;
    colors.table_active_border = selection_border;
    colors.table_row_border = control_border;

    colors.input = control_border;
    colors.button = control_fill;
    colors.button_foreground = text;
    colors.button_hover = hover_fill;
    colors.button_active = active_fill;
    colors.drag_border = active_border;
    colors.accent = accent;
    colors.accent_foreground = text;
    colors.primary = accent;
    colors.primary_foreground = text;
    colors.primary_hover = hover_fill;
    colors.primary_active = active_fill;
    colors.button_primary = accent;
    colors.button_primary_foreground = text;
    colors.button_primary_hover = hover_fill;
    colors.button_primary_active = active_fill;
    colors.ring = focus;
    colors.caret = accent;
    colors.selection = selection;
    colors.link = link;
    colors.link_hover = link;
    colors.link_active = link;

    colors.success = success;
    colors.success_foreground = text;
    colors.button_success = success;
    colors.button_success_foreground = text;
    colors.warning = warning;
    colors.warning_foreground = text;
    colors.button_warning = warning;
    colors.button_warning_foreground = text;
    colors.danger = error;
    colors.danger_foreground = text;
    colors.button_danger = error;
    colors.button_danger_foreground = text;

    colors.drop_target = hover_fill;
    colors.title_bar = window;
    colors.title_bar_border = control_border;
    colors.status_bar = panel;
    colors.status_bar_border = control_border;
}

pub(crate) fn to_hsla(color: ThemeColor) -> Hsla {
    Hsla::from(Rgba {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: f32::from(color.a) / 255.0,
    })
}

#[cfg(test)]
mod tests {
    use super::{apply_theme_values, kit_mode, should_observe_system, to_hsla};
    use gpui_kit::component::theme::{Theme as KitTheme, ThemeMode as KitThemeMode};
    use gpui_kit::{Hsla, WindowAppearance, px};
    use multi_launcher::{
        gui::theme::{SystemAppearance, resolve_theme},
        settings::{ColorScheme, ThemeColor, ThemeDensity, ThemeMode, ThemeSettings},
    };

    fn color(r: u8, g: u8, b: u8, a: u8) -> ThemeColor {
        ThemeColor { r, g, b, a }
    }

    #[test]
    fn fixed_and_system_modes_resolve_to_the_kit_mode() {
        let mut settings = ThemeSettings::default();
        settings.mode = ThemeMode::Light;
        let fixed_light = resolve_theme(&settings, SystemAppearance::Dark);
        assert_eq!(kit_mode(fixed_light.appearance), KitThemeMode::Light);

        settings.mode = ThemeMode::Dark;
        let fixed_dark = resolve_theme(&settings, SystemAppearance::Light);
        assert_eq!(kit_mode(fixed_dark.appearance), KitThemeMode::Dark);

        settings.mode = ThemeMode::System;
        assert_eq!(
            kit_mode(resolve_theme(&settings, SystemAppearance::Light).appearance),
            KitThemeMode::Light
        );
        assert_eq!(
            kit_mode(resolve_theme(&settings, SystemAppearance::Dark).appearance),
            KitThemeMode::Dark
        );
        assert!(should_observe_system(&settings));
        settings.mode = ThemeMode::Light;
        assert!(!should_observe_system(&settings));
        assert_eq!(
            super::system_appearance(WindowAppearance::VibrantDark),
            SystemAppearance::Dark
        );
        assert_eq!(
            super::system_appearance(WindowAppearance::VibrantLight),
            SystemAppearance::Light
        );
    }

    #[test]
    fn custom_fallback_and_semantic_colors_keep_their_alpha_and_identity() {
        let mut settings = ThemeSettings::default();
        settings.mode = ThemeMode::Custom;
        settings.custom_scheme = ColorScheme::dark();
        settings.custom_scheme.window_fill = color(18, 27, 39, 173);
        settings.custom_scheme.panel_fill = color(31, 43, 59, 191);
        settings.custom_scheme.hyperlink = color(48, 113, 201, 207);
        settings.custom_scheme.selection_bg = color(71, 129, 207, 121);
        settings.custom_scheme.selection_stroke = color(103, 156, 229, 223);
        settings.custom_scheme.success_accent = color(45, 183, 107, 211);
        settings.custom_scheme.warn_accent = color(232, 163, 58, 197);
        settings.custom_scheme.error_accent = color(224, 75, 88, 205);
        settings.preferences.active_preset = Some("missing preset".to_owned());
        let accent = color(131, 82, 197, 147);
        settings.preferences.accent = Some(accent);

        let resolved = resolve_theme(&settings, SystemAppearance::Light);
        assert_eq!(
            resolved.palette.window_surface,
            settings.custom_scheme.window_fill
        );
        assert_eq!(resolved.palette.hyperlink, settings.custom_scheme.hyperlink);
        assert_eq!(
            resolved.palette.selection_fill,
            settings.custom_scheme.selection_bg
        );
        assert_eq!(resolved.palette.accent, accent);

        let mut kit = KitTheme::default();
        apply_theme_values(&mut kit, resolved);
        assert_eq!(
            kit.colors.background,
            to_hsla(settings.custom_scheme.window_fill)
        );
        assert_eq!(
            kit.colors.sidebar,
            to_hsla(settings.custom_scheme.panel_fill)
        );
        assert_eq!(kit.colors.accent, to_hsla(accent));
        assert_eq!(kit.colors.link, to_hsla(settings.custom_scheme.hyperlink));
        assert_eq!(
            kit.colors.selection,
            to_hsla(settings.custom_scheme.selection_bg)
        );
        assert_eq!(
            kit.colors.list_active_border,
            to_hsla(settings.custom_scheme.selection_stroke)
        );
        assert_eq!(
            kit.colors.success,
            to_hsla(settings.custom_scheme.success_accent)
        );
        assert_eq!(
            kit.colors.warning,
            to_hsla(settings.custom_scheme.warn_accent)
        );
        assert_eq!(
            kit.colors.danger,
            to_hsla(settings.custom_scheme.error_accent)
        );
        assert_ne!(kit.colors.accent, kit.colors.link);
        assert_ne!(kit.colors.accent, kit.colors.selection);
        assert!((kit.colors.selection.a - 121.0 / 255.0).abs() < f32::EPSILON);
        assert_eq!(
            settings.preferences.active_preset.as_deref(),
            Some("missing preset")
        );
    }

    #[test]
    fn density_and_typography_metrics_reach_kit_without_rewriting_scale() {
        let mut settings = ThemeSettings::default();
        settings.preferences.density = ThemeDensity::Comfortable;
        settings.preferences.typography_scale = 1.5;
        let resolved = resolve_theme(&settings, SystemAppearance::Unknown);
        let mut kit = KitTheme::default();
        apply_theme_values(&mut kit, resolved);
        assert_eq!(resolved.metrics.item_spacing_x, 7.5);
        assert_eq!(resolved.metrics.item_spacing_y, 5.0);
        assert_eq!(resolved.metrics.corner_radius, 7.0);
        assert_eq!(resolved.metrics.row_min_height, 52.5);
        assert_eq!(kit.font_size, px(24.0));
        assert_eq!(kit.mono_font_size, px(19.5));
        assert_eq!(kit.radius, px(7.0));
        assert_eq!(settings.preferences.typography_scale, 1.5);

        settings.preferences.typography_scale = f32::NAN;
        let invalid = resolve_theme(&settings, SystemAppearance::Unknown);
        let mut invalid_kit = KitTheme::default();
        apply_theme_values(&mut invalid_kit, invalid);
        assert_eq!(invalid_kit.font_size, px(16.0));
        assert!(settings.preferences.typography_scale.is_nan());
        assert_eq!(invalid.metrics.body_text_size, 14.0);
    }

    #[test]
    fn saved_color_conversion_preserves_alpha() {
        let source = color(21, 77, 131, 69);
        let converted: Hsla = to_hsla(source);
        assert!((converted.a - 69.0 / 255.0).abs() < f32::EPSILON);
    }
}
