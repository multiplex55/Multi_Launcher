use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::hotkey::Hotkey;

const FIRST_PASS_TRANSPARENCY_COLORKEY: DrawColor = DrawColor::rgba(255, 0, 255, 255);
const COLORKEY_SAFE_FALLBACK: DrawColor = DrawColor::rgba(254, 0, 255, 255);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolbarPosition {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DrawTool {
    Pen,
    Line,
    Rect,
    Ellipse,
    Eraser,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CanvasBackgroundMode {
    Transparent,
    Solid,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LiveBackgroundMode {
    Transparent,
    Blank { color: DrawColor },
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum LegacyLiveBackgroundMode {
    DesktopTransparent,
    SolidColor,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum LiveBackgroundModeWire {
    New(LiveBackgroundMode),
    Legacy(LegacyLiveBackgroundMode),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DrawColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl DrawColor {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_rgba_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn from_rgba_array(color: [u8; 4]) -> Self {
        Self::rgba(color[0], color[1], color[2], color[3])
    }

    pub fn collides_with_first_pass_colorkey(self) -> bool {
        self.r == FIRST_PASS_TRANSPARENCY_COLORKEY.r
            && self.g == FIRST_PASS_TRANSPARENCY_COLORKEY.g
            && self.b == FIRST_PASS_TRANSPARENCY_COLORKEY.b
    }

    pub fn resolve_first_pass_colorkey_collision(self) -> Self {
        if self.collides_with_first_pass_colorkey() {
            COLORKEY_SAFE_FALLBACK
        } else {
            self
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DrawSettings {
    #[serde(default = "default_enable_pressure")]
    pub enable_pressure: bool,
    #[serde(default = "default_toolbar_position")]
    pub toolbar_position: ToolbarPosition,
    #[serde(default)]
    pub toolbar_collapsed: bool,
    #[serde(default = "default_toolbar_origin_x")]
    pub toolbar_origin_x: i32,
    #[serde(default = "default_toolbar_origin_y")]
    pub toolbar_origin_y: i32,
    #[serde(default = "default_toolbar_toggle_hotkey")]
    pub toolbar_toggle_hotkey: String,
    #[serde(default)]
    pub debug_hud_enabled: bool,
    #[serde(default = "default_debug_hud_toggle_hotkey")]
    pub debug_hud_toggle_hotkey: String,
    #[serde(default)]
    pub draw_perf_debug: bool,
    #[serde(default = "default_render_target_hz")]
    pub render_target_hz: u32,
    #[serde(default = "default_render_fallback_hz")]
    pub render_fallback_hz: u32,
    #[serde(default = "default_drop_intermediate_move_points_on_lag")]
    pub drop_intermediate_move_points_on_lag: bool,
    #[serde(default = "default_quick_colors")]
    pub quick_colors: Vec<DrawColor>,
    #[serde(default = "default_last_tool")]
    pub last_tool: DrawTool,
    #[serde(default = "default_last_color")]
    pub last_color: DrawColor,
    #[serde(default = "default_last_width")]
    pub last_width: u32,
    #[serde(default)]
    pub default_fill_enabled: bool,
    #[serde(default = "default_fill_color")]
    pub default_fill_color: DrawColor,
    #[serde(default = "default_outline_color")]
    pub default_outline_color: DrawColor,
    #[serde(default = "default_exit_timeout_seconds")]
    pub exit_timeout_seconds: u64,
    #[serde(default = "default_canvas_background_mode")]
    pub canvas_background_mode: CanvasBackgroundMode,
    #[serde(default = "default_blank_background_color")]
    pub canvas_solid_background_color: DrawColor,
    #[serde(default = "default_blank_background_color")]
    #[serde(alias = "export_background_color")]
    pub export_blank_background_color: DrawColor,
    #[serde(default = "default_offer_save_without_desktop")]
    pub offer_save_without_desktop: bool,
    #[serde(
        default = "default_fixed_save_folder_display",
        alias = "fixed_save_folder"
    )]
    pub fixed_save_folder_display: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DrawSettingsDe {
    #[serde(default = "default_enable_pressure")]
    enable_pressure: bool,
    #[serde(default = "default_toolbar_position")]
    toolbar_position: ToolbarPosition,
    #[serde(default)]
    toolbar_collapsed: bool,
    #[serde(default = "default_toolbar_origin_x")]
    toolbar_origin_x: i32,
    #[serde(default = "default_toolbar_origin_y")]
    toolbar_origin_y: i32,
    #[serde(default = "default_toolbar_toggle_hotkey")]
    toolbar_toggle_hotkey: String,
    #[serde(default)]
    debug_hud_enabled: bool,
    #[serde(default = "default_debug_hud_toggle_hotkey")]
    debug_hud_toggle_hotkey: String,
    #[serde(default)]
    draw_perf_debug: bool,
    #[serde(default = "default_render_target_hz")]
    render_target_hz: u32,
    #[serde(default = "default_render_fallback_hz")]
    render_fallback_hz: u32,
    #[serde(default = "default_drop_intermediate_move_points_on_lag")]
    drop_intermediate_move_points_on_lag: bool,
    #[serde(default = "default_quick_colors")]
    quick_colors: Vec<DrawColor>,
    #[serde(default = "default_last_tool")]
    last_tool: DrawTool,
    #[serde(default = "default_last_color")]
    last_color: DrawColor,
    #[serde(default = "default_last_width")]
    last_width: u32,
    #[serde(default)]
    default_fill_enabled: bool,
    #[serde(default = "default_fill_color")]
    default_fill_color: DrawColor,
    #[serde(default = "default_outline_color")]
    default_outline_color: DrawColor,
    #[serde(default = "default_exit_timeout_seconds")]
    exit_timeout_seconds: u64,
    #[serde(default)]
    canvas_background_mode: Option<CanvasBackgroundMode>,
    #[serde(default)]
    canvas_solid_background_color: Option<DrawColor>,
    #[serde(default)]
    live_background_mode: Option<LiveBackgroundModeWire>,
    #[serde(default)]
    live_blank_color: Option<DrawColor>,
    #[serde(
        default = "default_blank_background_color",
        alias = "export_background_color"
    )]
    export_blank_background_color: DrawColor,
    #[serde(default)]
    blank_background_color: Option<DrawColor>,
    #[serde(default = "default_offer_save_without_desktop")]
    offer_save_without_desktop: bool,
    #[serde(
        default = "default_fixed_save_folder_display",
        alias = "fixed_save_folder"
    )]
    fixed_save_folder_display: String,
}

impl<'de> Deserialize<'de> for DrawSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let decoded = DrawSettingsDe::deserialize(deserializer)?;
        let legacy_blank = decoded.blank_background_color;
        let mut live_blank = decoded
            .canvas_solid_background_color
            .or(decoded.live_blank_color)
            .or(legacy_blank)
            .unwrap_or_else(default_live_blank_color);
        let canvas_background_mode = match decoded.canvas_background_mode {
            Some(mode) => mode,
            None => match decoded.live_background_mode {
                Some(LiveBackgroundModeWire::New(mode)) => match mode {
                    LiveBackgroundMode::Transparent => CanvasBackgroundMode::Transparent,
                    LiveBackgroundMode::Blank { color } => {
                        live_blank = color;
                        CanvasBackgroundMode::Solid
                    }
                },
                Some(LiveBackgroundModeWire::Legacy(
                    LegacyLiveBackgroundMode::DesktopTransparent,
                )) => CanvasBackgroundMode::Transparent,
                Some(LiveBackgroundModeWire::Legacy(LegacyLiveBackgroundMode::SolidColor)) => {
                    CanvasBackgroundMode::Solid
                }
                None => default_canvas_background_mode(),
            },
        };
        Ok(Self {
            enable_pressure: decoded.enable_pressure,
            toolbar_position: decoded.toolbar_position,
            toolbar_collapsed: decoded.toolbar_collapsed,
            toolbar_origin_x: decoded.toolbar_origin_x,
            toolbar_origin_y: decoded.toolbar_origin_y,
            toolbar_toggle_hotkey: decoded.toolbar_toggle_hotkey,
            debug_hud_enabled: decoded.debug_hud_enabled,
            debug_hud_toggle_hotkey: decoded.debug_hud_toggle_hotkey,
            draw_perf_debug: decoded.draw_perf_debug,
            render_target_hz: decoded.render_target_hz,
            render_fallback_hz: decoded.render_fallback_hz,
            drop_intermediate_move_points_on_lag: decoded.drop_intermediate_move_points_on_lag,
            quick_colors: decoded.quick_colors,
            last_tool: decoded.last_tool,
            last_color: decoded.last_color,
            last_width: decoded.last_width,
            default_fill_enabled: decoded.default_fill_enabled,
            default_fill_color: decoded.default_fill_color,
            default_outline_color: decoded.default_outline_color,
            exit_timeout_seconds: decoded.exit_timeout_seconds,
            canvas_background_mode,
            canvas_solid_background_color: live_blank,
            export_blank_background_color: legacy_blank
                .unwrap_or(decoded.export_blank_background_color),
            offer_save_without_desktop: decoded.offer_save_without_desktop,
            fixed_save_folder_display: decoded.fixed_save_folder_display,
        })
    }
}

fn default_enable_pressure() -> bool {
    true
}

fn default_toolbar_position() -> ToolbarPosition {
    ToolbarPosition::Top
}

fn default_toolbar_origin_x() -> i32 {
    16
}

fn default_toolbar_origin_y() -> i32 {
    16
}

fn default_toolbar_toggle_hotkey() -> String {
    "Ctrl+Shift+D".to_owned()
}

fn default_debug_hud_toggle_hotkey() -> String {
    "Ctrl+Shift+H".to_owned()
}

pub fn default_toolbar_toggle_hotkey_value() -> String {
    default_toolbar_toggle_hotkey()
}

pub fn default_debug_hud_toggle_hotkey_value() -> String {
    default_debug_hud_toggle_hotkey()
}

fn default_last_tool() -> DrawTool {
    DrawTool::Pen
}

fn default_render_target_hz() -> u32 {
    120
}

fn default_render_fallback_hz() -> u32 {
    60
}

fn default_drop_intermediate_move_points_on_lag() -> bool {
    true
}

fn default_last_color() -> DrawColor {
    DrawColor::rgba(255, 255, 255, 255)
}

fn default_last_width() -> u32 {
    4
}

fn default_fill_color() -> DrawColor {
    DrawColor::rgba(0, 170, 255, 96)
}

fn default_outline_color() -> DrawColor {
    DrawColor::rgba(255, 255, 255, 255)
}

fn default_exit_timeout_seconds() -> u64 {
    120
}

fn default_blank_background_color() -> DrawColor {
    DrawColor::rgba(15, 18, 24, 255)
}

fn default_canvas_background_mode() -> CanvasBackgroundMode {
    CanvasBackgroundMode::Transparent
}

fn default_live_blank_color() -> DrawColor {
    default_blank_background_color()
}

fn default_offer_save_without_desktop() -> bool {
    true
}

fn default_fixed_save_folder_display() -> String {
    "Pictures/Multi Launcher/Draw".to_owned()
}

fn default_quick_colors() -> Vec<DrawColor> {
    vec![
        DrawColor::rgba(255, 255, 255, 255),
        DrawColor::rgba(0, 0, 0, 255),
        DrawColor::rgba(255, 64, 64, 255),
        DrawColor::rgba(255, 171, 0, 255),
        DrawColor::rgba(255, 230, 64, 255),
        DrawColor::rgba(61, 220, 132, 255),
        DrawColor::rgba(0, 168, 255, 255),
        DrawColor::rgba(180, 102, 255, 255),
    ]
}

impl Default for DrawSettings {
    fn default() -> Self {
        Self {
            enable_pressure: default_enable_pressure(),
            toolbar_position: default_toolbar_position(),
            toolbar_collapsed: false,
            toolbar_origin_x: default_toolbar_origin_x(),
            toolbar_origin_y: default_toolbar_origin_y(),
            toolbar_toggle_hotkey: default_toolbar_toggle_hotkey(),
            debug_hud_enabled: false,
            debug_hud_toggle_hotkey: default_debug_hud_toggle_hotkey(),
            draw_perf_debug: false,
            render_target_hz: default_render_target_hz(),
            render_fallback_hz: default_render_fallback_hz(),
            drop_intermediate_move_points_on_lag: default_drop_intermediate_move_points_on_lag(),
            quick_colors: default_quick_colors(),
            last_tool: default_last_tool(),
            last_color: default_last_color(),
            last_width: default_last_width(),
            default_fill_enabled: false,
            default_fill_color: default_fill_color(),
            default_outline_color: default_outline_color(),
            exit_timeout_seconds: default_exit_timeout_seconds(),
            canvas_background_mode: default_canvas_background_mode(),
            canvas_solid_background_color: default_blank_background_color(),
            export_blank_background_color: default_blank_background_color(),
            offer_save_without_desktop: default_offer_save_without_desktop(),
            fixed_save_folder_display: default_fixed_save_folder_display(),
        }
    }
}

impl DrawSettings {
    pub fn resolved_render_hz(&self) -> u32 {
        let target = self.render_target_hz.min(240);
        let fallback = self.render_fallback_hz.max(1).min(240);
        if target == 0 {
            fallback
        } else {
            target.max(1)
        }
    }

    pub fn tick_interval(&self) -> Duration {
        let hz = self.resolved_render_hz().max(1);
        Duration::from_secs_f64(1.0 / hz as f64)
    }

    pub fn parse_toolbar_toggle_hotkey(&self) -> Result<Hotkey, String> {
        crate::hotkey::parse_hotkey(&self.toolbar_toggle_hotkey)
            .ok_or_else(|| "Invalid hotkey format (example: Ctrl+Shift+D).".to_string())
    }

    /// Live desktop transparency guard: selected pen colors must not collide with
    /// the reserved color key used by the legacy first-pass overlay pipeline.
    pub fn sanitize_for_first_pass_transparency(&mut self) -> bool {
        let mut changed = false;

        let next = self.last_color.resolve_first_pass_colorkey_collision();
        changed |= next != self.last_color;
        self.last_color = next;

        let next = self
            .default_outline_color
            .resolve_first_pass_colorkey_collision();
        changed |= next != self.default_outline_color;
        self.default_outline_color = next;

        for quick in &mut self.quick_colors {
            let next = quick.resolve_first_pass_colorkey_collision();
            changed |= next != *quick;
            *quick = next;
        }

        let next = self
            .canvas_solid_background_color
            .resolve_first_pass_colorkey_collision();
        changed |= next != self.canvas_solid_background_color;
        self.canvas_solid_background_color = next;

        changed
    }

    pub fn toolbar_hotkey_valid(&self) -> bool {
        self.parse_toolbar_toggle_hotkey().is_ok()
    }

    pub fn sanitize_toolbar_hotkey_or_default(&mut self) -> bool {
        if self.toolbar_hotkey_valid() {
            return false;
        }
        self.toolbar_toggle_hotkey = default_toolbar_toggle_hotkey();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{CanvasBackgroundMode, DrawColor, DrawSettings};
    use crate::hotkey::Key;

    #[test]
    fn serde_roundtrip_draw_settings() {
        let settings = DrawSettings::default();
        let json = serde_json::to_string(&settings).expect("serialize draw settings");
        let decoded: DrawSettings = serde_json::from_str(&json).expect("deserialize draw settings");
        assert_eq!(decoded, settings);
    }

    #[test]
    fn defaults_cover_toolbar_pressure_and_save_behavior() {
        let settings = DrawSettings::default();
        assert!(settings.enable_pressure);
        assert_eq!(settings.toolbar_position, super::ToolbarPosition::Top);
        assert_eq!(settings.toolbar_toggle_hotkey, "Ctrl+Shift+D");
        assert!(!settings.debug_hud_enabled);
        assert_eq!(settings.debug_hud_toggle_hotkey, "Ctrl+Shift+H");
        assert_eq!(settings.render_target_hz, 120);
        assert_eq!(settings.render_fallback_hz, 60);
        assert!(settings.drop_intermediate_move_points_on_lag);
        assert!(settings.offer_save_without_desktop);
        assert_eq!(
            settings.fixed_save_folder_display,
            "Pictures/Multi Launcher/Draw"
        );
    }

    #[test]
    fn defaults_include_expected_timeout_background_and_quick_colors() {
        let settings = DrawSettings::default();
        assert_eq!(settings.exit_timeout_seconds, 120);
        assert_eq!(
            settings.canvas_background_mode,
            CanvasBackgroundMode::Transparent
        );
        assert_eq!(
            settings.canvas_solid_background_color,
            super::DrawColor::rgba(15, 18, 24, 255)
        );
        assert_eq!(
            settings.export_blank_background_color,
            super::DrawColor::rgba(15, 18, 24, 255)
        );
        assert_eq!(settings.quick_colors.len(), 8);
        assert_eq!(
            settings.quick_colors[0],
            super::DrawColor::rgba(255, 255, 255, 255)
        );
        assert_eq!(
            settings.quick_colors[1],
            super::DrawColor::rgba(0, 0, 0, 255)
        );
    }

    #[test]
    fn first_pass_transparency_guard_resolves_colorkey_collision() {
        let mut settings = DrawSettings::default();
        settings.last_color = DrawColor::rgba(255, 0, 255, 32);
        settings.default_outline_color = DrawColor::rgba(255, 0, 255, 255);
        settings.quick_colors[0] = DrawColor::rgba(255, 0, 255, 255);

        let changed = settings.sanitize_for_first_pass_transparency();

        assert!(changed);
        assert_eq!(settings.last_color, DrawColor::rgba(254, 0, 255, 255));
        assert_eq!(
            settings.default_outline_color,
            DrawColor::rgba(254, 0, 255, 255)
        );
        assert_eq!(settings.quick_colors[0], DrawColor::rgba(254, 0, 255, 255));
    }

    #[test]
    fn settings_roundtrip_canvas_background_mode_and_color() {
        let mut settings = DrawSettings::default();
        settings.canvas_background_mode = CanvasBackgroundMode::Solid;
        settings.canvas_solid_background_color = DrawColor::rgba(22, 33, 44, 255);
        settings.export_blank_background_color = DrawColor::rgba(1, 2, 3, 255);

        let json = serde_json::to_string(&settings).expect("serialize draw settings");
        let decoded: DrawSettings = serde_json::from_str(&json).expect("deserialize settings");

        assert_eq!(decoded.canvas_background_mode, CanvasBackgroundMode::Solid);
        assert_eq!(
            decoded.canvas_solid_background_color,
            DrawColor::rgba(22, 33, 44, 255)
        );
        assert_eq!(
            decoded.export_blank_background_color,
            DrawColor::rgba(1, 2, 3, 255)
        );
    }

    #[test]
    fn serde_roundtrip_preserves_fixed_folder_and_background_mode_values() {
        let mut settings = DrawSettings::default();
        settings.fixed_save_folder_display = "~/Pictures/Annotated".to_string();
        settings.canvas_background_mode = CanvasBackgroundMode::Solid;
        settings.canvas_solid_background_color = DrawColor::rgba(5, 15, 25, 255);

        let json = serde_json::to_string(&settings).expect("serialize draw settings");
        let decoded: DrawSettings = serde_json::from_str(&json).expect("deserialize draw settings");

        assert_eq!(decoded.fixed_save_folder_display, "~/Pictures/Annotated");
        assert_eq!(decoded.canvas_background_mode, CanvasBackgroundMode::Solid);
        assert_eq!(
            decoded.canvas_solid_background_color,
            DrawColor::rgba(5, 15, 25, 255)
        );
    }

    #[test]
    fn deserialize_legacy_fixed_save_folder_alias() {
        let decoded: DrawSettings = serde_json::from_value(serde_json::json!({
            "fixed_save_folder": "C:/Temp/Draw"
        }))
        .expect("deserialize draw settings");

        assert_eq!(decoded.fixed_save_folder_display, "C:/Temp/Draw");
    }

    #[test]
    fn canvas_background_mode_serializes_as_snake_case_enum() {
        let transparent_json =
            serde_json::to_value(CanvasBackgroundMode::Transparent).expect("serialize transparent");
        let solid_json =
            serde_json::to_value(CanvasBackgroundMode::Solid).expect("serialize solid");

        assert_eq!(transparent_json, serde_json::json!("transparent"));
        assert_eq!(solid_json, serde_json::json!("solid"));

        let decoded_transparent: CanvasBackgroundMode =
            serde_json::from_value(transparent_json).expect("deserialize transparent");
        let decoded_solid: CanvasBackgroundMode =
            serde_json::from_value(solid_json).expect("deserialize solid");

        assert_eq!(decoded_transparent, CanvasBackgroundMode::Transparent);
        assert_eq!(decoded_solid, CanvasBackgroundMode::Solid);
    }

    #[test]
    fn deserialize_legacy_blank_background_color_migrates_to_live_and_export() {
        let decoded: DrawSettings = serde_json::from_value(serde_json::json!({
            "blank_background_color": { "r": 9, "g": 8, "b": 7, "a": 255 }
        }))
        .expect("deserialize legacy draw settings");

        assert_eq!(
            decoded.canvas_background_mode,
            CanvasBackgroundMode::Transparent
        );
        assert_eq!(
            decoded.canvas_solid_background_color,
            DrawColor::rgba(9, 8, 7, 255)
        );
        assert_eq!(
            decoded.export_blank_background_color,
            DrawColor::rgba(9, 8, 7, 255)
        );
    }

    #[test]
    fn deserialize_tagged_live_background_mode_blank_preserves_color() {
        let decoded: DrawSettings = serde_json::from_value(serde_json::json!({
            "live_background_mode": {
                "kind": "blank",
                "color": { "r": 44, "g": 55, "b": 66, "a": 255 }
            }
        }))
        .expect("deserialize legacy tagged draw settings");

        assert_eq!(decoded.canvas_background_mode, CanvasBackgroundMode::Solid);
        assert_eq!(
            decoded.canvas_solid_background_color,
            DrawColor::rgba(44, 55, 66, 255)
        );
    }

    #[test]
    fn deserialize_legacy_live_background_mode_solid_color() {
        let decoded: DrawSettings = serde_json::from_value(serde_json::json!({
            "live_background_mode": "solid_color",
            "live_blank_color": { "r": 12, "g": 34, "b": 56, "a": 255 }
        }))
        .expect("deserialize legacy draw settings");

        assert_eq!(decoded.canvas_background_mode, CanvasBackgroundMode::Solid);
        assert_eq!(
            decoded.canvas_solid_background_color,
            DrawColor::rgba(12, 34, 56, 255)
        );
    }

    #[test]
    fn toolbar_hotkey_parser_accepts_common_combos() {
        let mut settings = DrawSettings::default();
        settings.toolbar_toggle_hotkey = "Ctrl+Alt+1".to_string();
        let parsed = settings
            .parse_toolbar_toggle_hotkey()
            .expect("hotkey should parse");
        assert_eq!(parsed.key, Key::Num1);
        assert!(parsed.ctrl);
        assert!(parsed.alt);
        assert!(!parsed.shift);
    }

    #[test]
    fn toolbar_hotkey_parser_rejects_malformed_or_unknown_keys() {
        let mut settings = DrawSettings::default();
        settings.toolbar_toggle_hotkey = "Ctrl++D".to_string();
        assert!(settings.parse_toolbar_toggle_hotkey().is_err());

        settings.toolbar_toggle_hotkey = "Ctrl+Shift+NotAKey".to_string();
        assert!(settings.parse_toolbar_toggle_hotkey().is_err());
    }
}
