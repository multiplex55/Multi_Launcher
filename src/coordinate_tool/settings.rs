use super::model::{CoordinateSample, CoordinateSpace, PhysicalPoint, PhysicalSize};
use serde::{Deserialize, Deserializer, Serialize};

/// Cursor-relative physical-pixel displacement for the HUD.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CoordinateOffset {
    pub x: i32,
    pub y: i32,
}

impl CoordinateOffset {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub const fn as_point(self) -> PhysicalPoint {
        PhysicalPoint::new(self.x, self.y)
    }
}

impl Default for CoordinateOffset {
    fn default() -> Self {
        Self::new(16, 24)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudDetail {
    #[default]
    Compact,
    Detailed,
}

/// RGB color for the crosshair. Opacity is configured independently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrosshairColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl CrosshairColor {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

impl Default for CrosshairColor {
    fn default() -> Self {
        Self::new(255, 0, 0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrosshairPreferences {
    pub color: CrosshairColor,
    /// Physical-pixel line thickness before display scaling.
    pub thickness: i32,
    /// Physical-pixel length of the colored part of each arm.
    pub arm_length: i32,
    /// Axial physical-pixel distance from the hotspot pixel center to the
    /// nearest visible pixel center, including the contrasting outline. A
    /// positive value N places the innermost visible pixels N pixel indices
    /// from the hotspot; zero lets each arm reach the hotspot pixel. Odd stroke
    /// thickness centers on the hotspot row/column, while even thickness keeps
    /// the renderer's existing upper/left rasterization bias.
    pub center_gap: i32,
    pub opacity: f32,
    pub virtual_desktop_guides: bool,
    pub high_contrast_outline: bool,
}

impl Default for CrosshairPreferences {
    fn default() -> Self {
        Self {
            color: CrosshairColor::default(),
            thickness: 2,
            arm_length: 12,
            center_gap: 16,
            opacity: 1.0,
            virtual_desktop_guides: false,
            high_contrast_outline: true,
        }
    }
}

/// Persisted preferences for the inspector and crosshair.
///
/// Enabled state, frozen samples, and copy status belong to
/// `CoordinateToolRuntimeState` and are deliberately excluded from settings.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CoordinateToolPreferences {
    pub space: CoordinateSpace,
    pub hud_detail: HudDetail,
    pub cursor_offset: CoordinateOffset,
    pub crosshair: CrosshairPreferences,
}

impl Default for CoordinateToolPreferences {
    fn default() -> Self {
        Self {
            space: CoordinateSpace::Desktop,
            hud_detail: HudDetail::Compact,
            cursor_offset: CoordinateOffset::default(),
            crosshair: CrosshairPreferences::default(),
        }
    }
}

impl CoordinateToolPreferences {
    pub fn normalized(mut self) -> Self {
        self.cursor_offset.x = self.cursor_offset.x.clamp(-512, 512);
        self.cursor_offset.y = self.cursor_offset.y.clamp(-512, 512);
        self.crosshair.thickness = self.crosshair.thickness.clamp(1, 16);
        self.crosshair.arm_length = self.crosshair.arm_length.clamp(2, 256);
        self.crosshair.center_gap = self.crosshair.center_gap.clamp(0, 128);
        self.crosshair.opacity = if self.crosshair.opacity.is_finite() {
            self.crosshair.opacity.clamp(0.1, 1.0)
        } else {
            CrosshairPreferences::default().opacity
        };
        self
    }

    /// Compute a work-area-clamped HUD origin from the same monitor information
    /// that accompanied the supplied cursor sample.
    pub fn hud_origin(
        &self,
        sample: &CoordinateSample,
        hud_size: PhysicalSize,
    ) -> Option<PhysicalPoint> {
        sample.hud_origin(self.cursor_offset.as_point(), hud_size)
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct RawCoordinateToolPreferences {
    space: CoordinateSpace,
    hud_detail: HudDetail,
    cursor_offset: CoordinateOffset,
    crosshair: CrosshairPreferences,
}

impl Default for RawCoordinateToolPreferences {
    fn default() -> Self {
        let preferences = CoordinateToolPreferences::default();
        Self {
            space: preferences.space,
            hud_detail: preferences.hud_detail,
            cursor_offset: preferences.cursor_offset,
            crosshair: preferences.crosshair,
        }
    }
}

impl<'de> Deserialize<'de> for CoordinateToolPreferences {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawCoordinateToolPreferences::deserialize(deserializer)?;
        Ok(Self {
            space: raw.space,
            hud_detail: raw.hud_detail,
            cursor_offset: raw.cursor_offset,
            crosshair: raw.crosshair,
        }
        .normalized())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CoordinateOffset, CoordinateToolPreferences, CrosshairColor, CrosshairPreferences,
        HudDetail,
    };
    use crate::coordinate_tool::model::CoordinateSpace;

    #[test]
    fn defaults_match_the_approved_inspector_and_crosshair_preferences() {
        let preferences = CoordinateToolPreferences::default();
        assert_eq!(preferences.space, CoordinateSpace::Desktop);
        assert_eq!(preferences.hud_detail, HudDetail::Compact);
        assert_eq!(preferences.cursor_offset, CoordinateOffset::new(16, 24));
        assert_eq!(preferences.crosshair.color, CrosshairColor::new(255, 0, 0));
        assert_eq!(preferences.crosshair.thickness, 2);
        assert_eq!(preferences.crosshair.arm_length, 12);
        assert_eq!(preferences.crosshair.center_gap, 16);
        assert_eq!(preferences.crosshair.opacity, 1.0);
        assert!(!preferences.crosshair.virtual_desktop_guides);
        assert!(preferences.crosshair.high_contrast_outline);
    }

    #[test]
    fn legacy_and_partial_preference_objects_keep_defaults_and_normalize_ranges() {
        let legacy: CoordinateToolPreferences = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy, CoordinateToolPreferences::default());

        let legacy_crosshair: CoordinateToolPreferences =
            serde_json::from_str(r#"{"crosshair":{"thickness":6,"arm_length":18}}"#).unwrap();
        assert_eq!(legacy_crosshair.crosshair.thickness, 6);
        assert_eq!(legacy_crosshair.crosshair.arm_length, 18);
        assert_eq!(legacy_crosshair.crosshair.center_gap, 16);

        let partial: CoordinateToolPreferences = serde_json::from_str(
            r#"{
                "space":"foreground_client",
                "hud_detail":"detailed",
                "cursor_offset":{"x":-900},
                "crosshair":{"thickness":99,"arm_length":1,"center_gap":400,"opacity":0.0}
            }"#,
        )
        .unwrap();
        assert_eq!(partial.space, CoordinateSpace::ForegroundClient);
        assert_eq!(partial.hud_detail, HudDetail::Detailed);
        assert_eq!(partial.cursor_offset, CoordinateOffset::new(-512, 24));
        assert_eq!(partial.crosshair.color, CrosshairColor::default());
        assert_eq!(partial.crosshair.thickness, 16);
        assert_eq!(partial.crosshair.arm_length, 2);
        assert_eq!(partial.crosshair.center_gap, 128);
        assert_eq!(partial.crosshair.opacity, 0.1);
        assert!(!partial.crosshair.virtual_desktop_guides);
        assert!(partial.crosshair.high_contrast_outline);
    }

    #[test]
    fn normalization_handles_out_of_range_offsets_and_non_finite_opacity() {
        let normalized = CoordinateToolPreferences {
            cursor_offset: CoordinateOffset::new(i32::MAX, i32::MIN),
            crosshair: CrosshairPreferences {
                thickness: i32::MIN,
                arm_length: i32::MAX,
                center_gap: i32::MIN,
                opacity: f32::NAN,
                ..Default::default()
            },
            ..Default::default()
        }
        .normalized();
        assert_eq!(normalized.cursor_offset, CoordinateOffset::new(512, -512));
        assert_eq!(normalized.crosshair.thickness, 1);
        assert_eq!(normalized.crosshair.arm_length, 256);
        assert_eq!(normalized.crosshair.center_gap, 0);
        assert_eq!(normalized.crosshair.opacity, 1.0);

        let maximum_gap = CoordinateToolPreferences {
            crosshair: CrosshairPreferences {
                center_gap: i32::MAX,
                ..Default::default()
            },
            ..Default::default()
        }
        .normalized();
        assert_eq!(maximum_gap.crosshair.center_gap, 128);
    }

    #[test]
    fn preferences_round_trip_without_transient_enabled_state() {
        let preferences = CoordinateToolPreferences {
            hud_detail: HudDetail::Detailed,
            crosshair: CrosshairPreferences {
                center_gap: 37,
                virtual_desktop_guides: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let value = serde_json::to_value(&preferences).unwrap();
        assert!(value.get("hud_enabled").is_none());
        assert!(value.get("crosshair_enabled").is_none());
        assert_eq!(
            serde_json::from_value::<CoordinateToolPreferences>(value).unwrap(),
            preferences
        );
    }
}
