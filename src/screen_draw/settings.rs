use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::geometry::DesktopPoint;
use super::model::{RgbaColor, ScreenDrawTool, ToolbarOrientation};

pub const PALETTE_SLOT_COUNT: usize = 24;
pub const DEFAULT_FADE_DURATION_SECONDS: u8 = 3;
pub const MIN_FADE_DURATION_SECONDS: u8 = 1;
pub const MAX_FADE_DURATION_SECONDS: u8 = 30;

/// A persisted chord using Multi Launcher's established hotkey string syntax.
///
/// Deserialization remains permissive so one malformed optional binding does
/// not make the entire settings entry unreadable. Call [`Self::is_valid`] or
/// [`ScreenDrawSettings::validate_hotkeys`] before registration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HotkeyChord(String);

impl HotkeyChord {
    pub fn new(value: impl Into<String>) -> Result<Self, SettingsValidationError> {
        let chord = Self(value.into());
        if chord.is_valid() {
            Ok(chord)
        } else {
            Err(SettingsValidationError::invalid_hotkey(
                "hotkey",
                chord.as_str(),
            ))
        }
    }

    pub fn from_unchecked(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_valid(&self) -> bool {
        crate::hotkey::parse_hotkey(self.as_str()).is_some()
    }
}

impl fmt::Display for HotkeyChord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsValidationError {
    pub field: String,
    pub message: String,
}

impl SettingsValidationError {
    fn invalid_hotkey(field: impl Into<String>, value: &str) -> Self {
        Self {
            field: field.into(),
            message: format!("invalid hotkey '{value}'"),
        }
    }
}

impl fmt::Display for SettingsValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.field, self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreenDrawSettings {
    pub default_tool: ScreenDrawTool,
    pub default_color: RgbaColor,
    pub default_thickness: f32,
    pub text_size: f32,
    pub highlighter_alpha: u8,
    pub fade_duration_seconds: u8,
    pub palette: [RgbaColor; PALETTE_SLOT_COUNT],
    pub toolbar_position: Option<DesktopPoint>,
    pub toolbar_orientation: ToolbarOrientation,
    pub launch_hotkey: Option<HotkeyChord>,
    pub emergency_hotkey: HotkeyChord,
    pub tool_hotkeys: BTreeMap<ScreenDrawTool, HotkeyChord>,
    pub increase_thickness_hotkey: Option<HotkeyChord>,
    pub decrease_thickness_hotkey: Option<HotkeyChord>,
    pub quick_color_hotkeys: [Option<HotkeyChord>; PALETTE_SLOT_COUNT],
}

impl Default for ScreenDrawSettings {
    fn default() -> Self {
        Self {
            default_tool: ScreenDrawTool::Pen,
            default_color: RgbaColor::RED,
            default_thickness: 3.0,
            text_size: 24.0,
            highlighter_alpha: 96,
            fade_duration_seconds: DEFAULT_FADE_DURATION_SECONDS,
            palette: default_palette(),
            toolbar_position: None,
            toolbar_orientation: ToolbarOrientation::Vertical,
            launch_hotkey: None,
            emergency_hotkey: HotkeyChord::from_unchecked("Ctrl+Shift+F12"),
            tool_hotkeys: default_tool_hotkeys(),
            increase_thickness_hotkey: None,
            decrease_thickness_hotkey: None,
            quick_color_hotkeys: default_quick_color_hotkeys(),
        }
    }
}

impl ScreenDrawSettings {
    /// Clamps user-editable numeric preferences to supported runtime ranges.
    pub fn normalize(&mut self) {
        self.default_thickness = finite_or(self.default_thickness, 3.0).clamp(0.5, 64.0);
        self.text_size = finite_or(self.text_size, 24.0).clamp(8.0, 256.0);
        self.fade_duration_seconds = self
            .fade_duration_seconds
            .clamp(MIN_FADE_DURATION_SECONDS, MAX_FADE_DURATION_SECONDS);
    }

    /// Reports every malformed configured chord without preventing unrelated
    /// preferences from loading.
    pub fn validate_hotkeys(&self) -> Vec<SettingsValidationError> {
        let mut errors = Vec::new();
        validate_optional_hotkey("launch_hotkey", self.launch_hotkey.as_ref(), &mut errors);
        validate_hotkey("emergency_hotkey", &self.emergency_hotkey, &mut errors);
        for (tool, hotkey) in &self.tool_hotkeys {
            validate_hotkey(format!("tool_hotkeys.{tool:?}"), hotkey, &mut errors);
        }
        validate_optional_hotkey(
            "increase_thickness_hotkey",
            self.increase_thickness_hotkey.as_ref(),
            &mut errors,
        );
        validate_optional_hotkey(
            "decrease_thickness_hotkey",
            self.decrease_thickness_hotkey.as_ref(),
            &mut errors,
        );
        for (index, hotkey) in self.quick_color_hotkeys.iter().enumerate() {
            validate_optional_hotkey(
                format!("quick_color_hotkeys.{index}"),
                hotkey.as_ref(),
                &mut errors,
            );
        }
        errors
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn validate_hotkey(
    field: impl Into<String>,
    hotkey: &HotkeyChord,
    errors: &mut Vec<SettingsValidationError>,
) {
    if !hotkey.is_valid() {
        errors.push(SettingsValidationError::invalid_hotkey(
            field,
            hotkey.as_str(),
        ));
    }
}

fn validate_optional_hotkey(
    field: impl Into<String>,
    hotkey: Option<&HotkeyChord>,
    errors: &mut Vec<SettingsValidationError>,
) {
    if let Some(hotkey) = hotkey {
        validate_hotkey(field, hotkey, errors);
    }
}

fn default_tool_hotkeys() -> BTreeMap<ScreenDrawTool, HotkeyChord> {
    use ScreenDrawTool::*;
    [
        (Pen, "P"),
        (Highlighter, "H"),
        (StraightLine, "L"),
        (Arrow, "A"),
        (Rectangle, "R"),
        (Ellipse, "O"),
        (Text, "T"),
        (Eraser, "E"),
        // The shared parser currently reserves the `F` prefix for function
        // keys, so use a distinct ordinary-letter default for fading ink.
        (FadingInk, "D"),
        (Eyedropper, "I"),
    ]
    .into_iter()
    .map(|(tool, chord)| (tool, HotkeyChord::from_unchecked(chord)))
    .collect()
}

fn default_quick_color_hotkeys() -> [Option<HotkeyChord>; PALETTE_SLOT_COUNT] {
    std::array::from_fn(|index| {
        (index < 9).then(|| HotkeyChord::from_unchecked((index + 1).to_string()))
    })
}

fn default_palette() -> [RgbaColor; PALETTE_SLOT_COUNT] {
    use RgbaColor as C;
    [
        C::rgba(255, 0, 0, 255),
        C::rgba(255, 128, 0, 255),
        C::rgba(255, 215, 0, 255),
        C::rgba(0, 180, 80, 255),
        C::rgba(0, 170, 255, 255),
        C::rgba(45, 90, 255, 255),
        C::rgba(135, 70, 255, 255),
        C::rgba(230, 50, 180, 255),
        C::rgba(255, 255, 255, 255),
        C::rgba(0, 0, 0, 255),
        C::rgba(128, 0, 0, 255),
        C::rgba(128, 64, 0, 255),
        C::rgba(128, 110, 0, 255),
        C::rgba(0, 100, 45, 255),
        C::rgba(0, 90, 140, 255),
        C::rgba(25, 45, 140, 255),
        C::rgba(75, 35, 140, 255),
        C::rgba(135, 25, 105, 255),
        C::rgba(220, 220, 220, 255),
        C::rgba(160, 160, 160, 255),
        C::rgba(110, 110, 110, 255),
        C::rgba(75, 75, 75, 255),
        C::rgba(40, 40, 40, 255),
        C::rgba(20, 20, 20, 255),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_include_safe_hotkey_fade_and_full_palette() {
        let settings = ScreenDrawSettings::default();
        assert_eq!(settings.launch_hotkey, None);
        assert_eq!(settings.emergency_hotkey.as_str(), "Ctrl+Shift+F12");
        assert!(settings.emergency_hotkey.is_valid());
        assert_eq!(settings.fade_duration_seconds, 3);
        assert_eq!(settings.palette.len(), 24);
        assert_eq!(settings.quick_color_hotkeys.len(), 24);
        assert!(settings.validate_hotkeys().is_empty());
    }

    #[test]
    fn missing_json_fields_receive_defaults_and_round_trip() {
        let settings: ScreenDrawSettings = serde_json::from_value(serde_json::json!({
            "default_color": [12, 34, 56, 255],
            "toolbar_position": { "x": -1700, "y": 40 }
        }))
        .unwrap();
        assert_eq!(settings.default_color, RgbaColor::rgba(12, 34, 56, 255));
        assert_eq!(settings.default_tool, ScreenDrawTool::Pen);
        assert_eq!(
            settings.toolbar_position,
            Some(DesktopPoint::new(-1700, 40))
        );

        let round_trip: ScreenDrawSettings =
            serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
        assert_eq!(round_trip, settings);
    }

    #[test]
    fn palette_is_exactly_twenty_four_persisted_slots() {
        let mut settings = ScreenDrawSettings::default();
        settings.palette[23] = RgbaColor::rgba(7, 8, 9, 10);
        let value = serde_json::to_value(&settings).unwrap();
        assert_eq!(value["palette"].as_array().unwrap().len(), 24);
        let restored: ScreenDrawSettings = serde_json::from_value(value).unwrap();
        assert_eq!(restored.palette[23], RgbaColor::rgba(7, 8, 9, 10));
    }

    #[test]
    fn hotkeys_use_shared_parser_and_report_invalid_persisted_values() {
        assert!(HotkeyChord::new("Ctrl + Shift + F12").is_ok());
        assert!(HotkeyChord::new("Ctrl+DefinitelyNotAKey").is_err());

        let mut settings = ScreenDrawSettings::default();
        settings.launch_hotkey = Some(HotkeyChord::from_unchecked("Ctrl+Nope"));
        settings.tool_hotkeys.insert(
            ScreenDrawTool::Pen,
            HotkeyChord::from_unchecked("Alt+NotAKey"),
        );
        let errors = settings.validate_hotkeys();
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().any(|error| error.field == "launch_hotkey"));
        assert!(errors.iter().any(|error| error.field == "tool_hotkeys.Pen"));
    }

    #[test]
    fn normalization_enforces_supported_numeric_ranges() {
        let mut settings = ScreenDrawSettings {
            default_thickness: f32::NAN,
            text_size: 1000.0,
            fade_duration_seconds: 0,
            ..Default::default()
        };
        settings.normalize();
        assert_eq!(settings.default_thickness, 3.0);
        assert_eq!(settings.text_size, 256.0);
        assert_eq!(settings.fade_duration_seconds, 1);
    }
}
