use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DrawSettings {
    #[serde(default = "default_enable_pressure")]
    pub enable_pressure: bool,
    #[serde(default = "default_toolbar_position")]
    pub toolbar_position: ToolbarPosition,
    #[serde(default)]
    pub toolbar_collapsed: bool,
    #[serde(default = "default_toolbar_toggle_hotkey")]
    pub toolbar_toggle_hotkey: String,
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
    #[serde(default = "default_blank_background_color")]
    pub blank_background_color: DrawColor,
    #[serde(default = "default_offer_save_without_desktop")]
    pub offer_save_without_desktop: bool,
    #[serde(default = "default_fixed_save_folder_display")]
    pub fixed_save_folder_display: String,
}

fn default_enable_pressure() -> bool {
    true
}

fn default_toolbar_position() -> ToolbarPosition {
    ToolbarPosition::Top
}

fn default_toolbar_toggle_hotkey() -> String {
    "Ctrl+Shift+D".to_owned()
}

fn default_last_tool() -> DrawTool {
    DrawTool::Pen
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
            toolbar_toggle_hotkey: default_toolbar_toggle_hotkey(),
            quick_colors: default_quick_colors(),
            last_tool: default_last_tool(),
            last_color: default_last_color(),
            last_width: default_last_width(),
            default_fill_enabled: false,
            default_fill_color: default_fill_color(),
            default_outline_color: default_outline_color(),
            exit_timeout_seconds: default_exit_timeout_seconds(),
            blank_background_color: default_blank_background_color(),
            offer_save_without_desktop: default_offer_save_without_desktop(),
            fixed_save_folder_display: default_fixed_save_folder_display(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DrawSettings;

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
            settings.blank_background_color,
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
}
