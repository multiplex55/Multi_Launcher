use crate::settings::{ColorScheme, ThemeColor, ThemeMode, ThemeSettings};
use eframe::egui;

fn theme_color_to_color32(color: ThemeColor) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

pub fn preset_for_mode(
    theme: &ThemeSettings,
    mode: ThemeMode,
) -> Result<ColorScheme, &'static str> {
    match mode {
        ThemeMode::Custom => Ok(theme.custom_scheme.clone()),
        ThemeMode::Dark => theme
            .named_presets
            .get("dark")
            .cloned()
            .ok_or("missing dark preset"),
        ThemeMode::Light => theme
            .named_presets
            .get("light")
            .cloned()
            .ok_or("missing light preset"),
        ThemeMode::System => Err("system mode uses context defaults"),
    }
}

pub fn theme_settings_to_visuals(theme: &ThemeSettings, defaults: &egui::Visuals) -> egui::Visuals {
    if matches!(theme.mode, ThemeMode::System) {
        return defaults.clone();
    }

    let Ok(scheme) = preset_for_mode(theme, theme.mode) else {
        return defaults.clone();
    };

    let mut visuals = defaults.clone();
    visuals.dark_mode = !matches!(theme.mode, ThemeMode::Light);
    visuals.window_fill = theme_color_to_color32(scheme.window_fill);
    visuals.panel_fill = theme_color_to_color32(scheme.panel_fill);
    visuals.override_text_color = Some(theme_color_to_color32(scheme.text));
    visuals.hyperlink_color = theme_color_to_color32(scheme.hyperlink);

    visuals.widgets.noninteractive.bg_fill = theme_color_to_color32(scheme.panel_fill);
    visuals.widgets.inactive.bg_fill = theme_color_to_color32(scheme.widget_inactive_fill);
    visuals.widgets.inactive.bg_stroke.color =
        theme_color_to_color32(scheme.widget_inactive_stroke);
    visuals.widgets.hovered.bg_fill = theme_color_to_color32(scheme.widget_hovered_fill);
    visuals.widgets.hovered.bg_stroke.color = theme_color_to_color32(scheme.widget_hovered_stroke);
    visuals.widgets.active.bg_fill = theme_color_to_color32(scheme.widget_active_fill);
    visuals.widgets.active.bg_stroke.color = theme_color_to_color32(scheme.widget_active_stroke);

    visuals.selection.bg_fill = theme_color_to_color32(scheme.selection_bg);
    visuals.selection.stroke.color = theme_color_to_color32(scheme.selection_stroke);
    visuals.warn_fg_color = theme_color_to_color32(scheme.warn_accent);
    visuals.error_fg_color = theme_color_to_color32(scheme.error_accent);

    visuals
}

#[cfg(test)]
mod tests {
    use super::{preset_for_mode, theme_settings_to_visuals};
    use crate::settings::{ColorScheme, ThemeMode, ThemeSettings};
    use eframe::egui;

    #[test]
    fn conversion_maps_known_colors() {
        let mut theme = ThemeSettings::default();
        theme.mode = ThemeMode::Custom;
        theme.custom_scheme = ColorScheme::light();

        let base = egui::Visuals::dark();
        let visuals = theme_settings_to_visuals(&theme, &base);

        assert_eq!(visuals.window_fill, egui::Color32::from_rgb(245, 246, 250));
        assert_eq!(visuals.panel_fill, egui::Color32::from_rgb(255, 255, 255));
        assert_eq!(
            visuals.hyperlink_color,
            egui::Color32::from_rgb(35, 102, 214)
        );
        assert_eq!(
            visuals.selection.bg_fill,
            egui::Color32::from_rgba_unmultiplied(153, 194, 255, 220)
        );
    }

    #[test]
    fn mode_switching_is_deterministic() {
        let theme = ThemeSettings::default();
        let base = egui::Visuals::light();

        let dark = theme_settings_to_visuals(
            &ThemeSettings {
                mode: ThemeMode::Dark,
                ..theme.clone()
            },
            &base,
        );
        let light = theme_settings_to_visuals(
            &ThemeSettings {
                mode: ThemeMode::Light,
                ..theme.clone()
            },
            &base,
        );
        let custom = theme_settings_to_visuals(
            &ThemeSettings {
                mode: ThemeMode::Custom,
                ..theme.clone()
            },
            &base,
        );
        let system = theme_settings_to_visuals(
            &ThemeSettings {
                mode: ThemeMode::System,
                ..theme.clone()
            },
            &base,
        );

        assert!(dark.dark_mode);
        assert!(!light.dark_mode);
        assert!(custom.dark_mode);
        assert_ne!(dark.window_fill, light.window_fill);
        assert_eq!(dark.window_fill, custom.window_fill);
        assert_eq!(system, base);
    }

    #[test]
    fn malformed_theme_falls_back_to_defaults() {
        let mut theme = ThemeSettings::default();
        theme.mode = ThemeMode::Dark;
        theme.named_presets.clear();

        let base = egui::Visuals::dark();
        let visuals = theme_settings_to_visuals(&theme, &base);

        assert_eq!(visuals, base);
        assert!(preset_for_mode(&theme, ThemeMode::Dark).is_err());
    }
}
