//! Pure resolution from persisted theme preferences to semantic UI values.
//!
//! This module deliberately does not read the operating system or an egui
//! `Context`, mutate settings, or apply styles. Runtime style ownership is
//! provided by a later milestone.

use crate::settings::{
    ColorScheme, ThemeColor, ThemeDensity, ThemeMode, ThemePreferences, ThemeSettings,
};

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
    let (appearance, scheme) = match settings.mode {
        ThemeMode::System => match system_appearance {
            SystemAppearance::Light => (ResolvedAppearance::Light, ColorScheme::light()),
            SystemAppearance::Dark | SystemAppearance::Unknown => {
                (ResolvedAppearance::Dark, ColorScheme::dark())
            }
        },
        ThemeMode::Dark => (ResolvedAppearance::Dark, ColorScheme::dark()),
        ThemeMode::Light => (ResolvedAppearance::Light, ColorScheme::light()),
        ThemeMode::Custom => {
            let selected = settings
                .preferences
                .active_preset
                .as_deref()
                .and_then(|name| settings.named_presets.get(name))
                .unwrap_or(&settings.custom_scheme);
            // Custom palettes retain their chosen colors. The appearance only
            // supplies a deterministic base preference for later runtime use.
            (ResolvedAppearance::Dark, selected.clone())
        }
    };

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

#[cfg(test)]
mod tests {
    use super::{ResolvedAppearance, SystemAppearance, resolve_theme};
    use crate::settings::{
        ColorScheme, ThemeColor, ThemeDensity, ThemeMode, ThemePreferences, ThemeSettings,
    };
    use std::collections::HashMap;

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
