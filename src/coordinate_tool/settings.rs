use super::model::{CoordinateSample, CoordinateSpace, PhysicalPoint, PhysicalSize};
use serde::{Deserialize, Deserializer, Serialize};

/// Signed physical-pixel displacement; the owning preference defines its
/// reference point.
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

/// RGB color for the crosshair or a cursor-effect outline. Crosshair opacity
/// is configured independently.
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

/// Appearance preferences for the live, cursor-centered inversion halo.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct HaloPreferences {
    /// Halo radius in physical pixels.
    pub radius: i32,
    /// Fraction of full color inversion, from 0.0 (unchanged) to 1.0 (full).
    pub inversion_strength: f32,
    pub outline_enabled: bool,
    pub outline_color: CrosshairColor,
    /// Outline thickness in physical pixels.
    pub outline_thickness: i32,
}

impl Default for HaloPreferences {
    fn default() -> Self {
        Self {
            radius: 60,
            inversion_strength: 0.4,
            outline_enabled: false,
            outline_color: CrosshairColor::new(255, 255, 255),
            outline_thickness: 1,
        }
    }
}

impl HaloPreferences {
    pub fn normalized(mut self) -> Self {
        self.radius = self.radius.clamp(8, 256);
        self.inversion_strength = if self.inversion_strength.is_finite() {
            self.inversion_strength.clamp(0.0, 1.0)
        } else {
            Self::default().inversion_strength
        };
        self.outline_thickness = self.outline_thickness.clamp(1, 8);
        self
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct RawHaloPreferences {
    radius: i32,
    inversion_strength: f32,
    outline_enabled: bool,
    outline_color: CrosshairColor,
    outline_thickness: i32,
}

impl Default for RawHaloPreferences {
    fn default() -> Self {
        let preferences = HaloPreferences::default();
        Self {
            radius: preferences.radius,
            inversion_strength: preferences.inversion_strength,
            outline_enabled: preferences.outline_enabled,
            outline_color: preferences.outline_color,
            outline_thickness: preferences.outline_thickness,
        }
    }
}

impl<'de> Deserialize<'de> for HaloPreferences {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawHaloPreferences::deserialize(deserializer)?;
        Ok(Self {
            radius: raw.radius,
            inversion_strength: raw.inversion_strength,
            outline_enabled: raw.outline_enabled,
            outline_color: raw.outline_color,
            outline_thickness: raw.outline_thickness,
        }
        .normalized())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoomMode {
    #[default]
    Offset,
    Centered,
}

/// Appearance preferences for the live, cursor-centered desktop magnifier.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ZoomPreferences {
    pub mode: ZoomMode,
    pub zoom_factor: f32,
    /// Circular lens diameter in physical pixels.
    pub diameter: i32,
    /// Destination displacement from the physical cursor hotspot. The source
    /// remains centered on the hotspot for either destination mode.
    pub destination_offset: CoordinateOffset,
    pub outline_enabled: bool,
    pub outline_color: CrosshairColor,
    /// Outline thickness in physical pixels.
    pub outline_thickness: i32,
}

impl Default for ZoomPreferences {
    fn default() -> Self {
        Self {
            mode: ZoomMode::Offset,
            zoom_factor: 2.0,
            diameter: 160,
            destination_offset: CoordinateOffset::new(120, 80),
            outline_enabled: true,
            outline_color: CrosshairColor::new(255, 255, 255),
            outline_thickness: 1,
        }
    }
}

impl ZoomPreferences {
    pub fn normalized(mut self) -> Self {
        self.zoom_factor = if self.zoom_factor.is_finite() {
            self.zoom_factor.clamp(1.25, 4.0)
        } else {
            Self::default().zoom_factor
        };
        self.diameter = self.diameter.clamp(64, 480);
        self.destination_offset.x = self.destination_offset.x.clamp(-2048, 2048);
        self.destination_offset.y = self.destination_offset.y.clamp(-2048, 2048);
        self.outline_thickness = self.outline_thickness.clamp(1, 8);
        self
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct RawZoomOffset {
    x: i32,
    y: i32,
}

impl Default for RawZoomOffset {
    fn default() -> Self {
        let offset = ZoomPreferences::default().destination_offset;
        Self {
            x: offset.x,
            y: offset.y,
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct RawZoomPreferences {
    mode: ZoomMode,
    zoom_factor: f32,
    diameter: i32,
    destination_offset: RawZoomOffset,
    outline_enabled: bool,
    outline_color: CrosshairColor,
    outline_thickness: i32,
}

impl Default for RawZoomPreferences {
    fn default() -> Self {
        let preferences = ZoomPreferences::default();
        Self {
            mode: preferences.mode,
            zoom_factor: preferences.zoom_factor,
            diameter: preferences.diameter,
            destination_offset: RawZoomOffset::default(),
            outline_enabled: preferences.outline_enabled,
            outline_color: preferences.outline_color,
            outline_thickness: preferences.outline_thickness,
        }
    }
}

impl<'de> Deserialize<'de> for ZoomPreferences {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawZoomPreferences::deserialize(deserializer)?;
        Ok(Self {
            mode: raw.mode,
            zoom_factor: raw.zoom_factor,
            diameter: raw.diameter,
            destination_offset: CoordinateOffset::new(
                raw.destination_offset.x,
                raw.destination_offset.y,
            ),
            outline_enabled: raw.outline_enabled,
            outline_color: raw.outline_color,
            outline_thickness: raw.outline_thickness,
        }
        .normalized())
    }
}

/// Persisted appearance preferences for the inspector and cursor effects.
///
/// HUD/crosshair/halo/zoom enabled state, frozen samples, and copy status belong
/// to `CoordinateToolRuntimeState` and are deliberately excluded from settings.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CoordinateToolPreferences {
    pub space: CoordinateSpace,
    pub hud_detail: HudDetail,
    pub cursor_offset: CoordinateOffset,
    pub crosshair: CrosshairPreferences,
    pub halo: HaloPreferences,
    pub zoom: ZoomPreferences,
}

impl Default for CoordinateToolPreferences {
    fn default() -> Self {
        Self {
            space: CoordinateSpace::Desktop,
            hud_detail: HudDetail::Compact,
            cursor_offset: CoordinateOffset::default(),
            crosshair: CrosshairPreferences::default(),
            halo: HaloPreferences::default(),
            zoom: ZoomPreferences::default(),
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
        self.halo = self.halo.normalized();
        self.zoom = self.zoom.normalized();
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
    halo: HaloPreferences,
    zoom: ZoomPreferences,
}

impl Default for RawCoordinateToolPreferences {
    fn default() -> Self {
        let preferences = CoordinateToolPreferences::default();
        Self {
            space: preferences.space,
            hud_detail: preferences.hud_detail,
            cursor_offset: preferences.cursor_offset,
            crosshair: preferences.crosshair,
            halo: preferences.halo,
            zoom: preferences.zoom,
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
            halo: raw.halo,
            zoom: raw.zoom,
        }
        .normalized())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CoordinateOffset, CoordinateToolPreferences, CrosshairColor, CrosshairPreferences,
        HaloPreferences, HudDetail, ZoomMode, ZoomPreferences,
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
        assert_eq!(preferences.halo, HaloPreferences::default());
        assert_eq!(preferences.zoom, ZoomPreferences::default());
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
        assert_eq!(legacy_crosshair.halo, HaloPreferences::default());
        assert_eq!(legacy_crosshair.zoom, ZoomPreferences::default());

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
    fn partial_effect_preferences_keep_effect_specific_defaults() {
        let partial: CoordinateToolPreferences = serde_json::from_str(
            r#"{
                "halo":{"radius":72},
                "zoom":{"mode":"centered","destination_offset":{"x":300}}
            }"#,
        )
        .unwrap();

        assert_eq!(partial.halo.radius, 72);
        assert_eq!(partial.halo.inversion_strength, 0.4);
        assert!(!partial.halo.outline_enabled);
        assert_eq!(
            partial.halo.outline_color,
            CrosshairColor::new(255, 255, 255)
        );
        assert_eq!(partial.halo.outline_thickness, 1);
        assert_eq!(partial.zoom.mode, ZoomMode::Centered);
        assert_eq!(partial.zoom.zoom_factor, 2.0);
        assert_eq!(partial.zoom.diameter, 160);
        assert_eq!(
            partial.zoom.destination_offset,
            CoordinateOffset::new(300, 80)
        );
        assert!(partial.zoom.outline_enabled);
        assert_eq!(
            partial.zoom.outline_color,
            CrosshairColor::new(255, 255, 255)
        );
        assert_eq!(partial.zoom.outline_thickness, 1);

        let empty_zoom_offset: ZoomPreferences =
            serde_json::from_str(r#"{"destination_offset":{}}"#).unwrap();
        assert_eq!(
            empty_zoom_offset.destination_offset,
            CoordinateOffset::new(120, 80)
        );
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
    fn effect_preferences_normalize_ranges_and_non_finite_values() {
        let normalized = CoordinateToolPreferences {
            halo: HaloPreferences {
                radius: i32::MAX,
                inversion_strength: f32::NAN,
                outline_thickness: i32::MIN,
                ..Default::default()
            },
            zoom: ZoomPreferences {
                zoom_factor: f32::INFINITY,
                diameter: i32::MIN,
                destination_offset: CoordinateOffset::new(i32::MAX, i32::MIN),
                outline_thickness: i32::MAX,
                ..Default::default()
            },
            ..Default::default()
        }
        .normalized();

        assert_eq!(normalized.halo.radius, 256);
        assert_eq!(normalized.halo.inversion_strength, 0.4);
        assert_eq!(normalized.halo.outline_thickness, 1);
        assert_eq!(normalized.zoom.zoom_factor, 2.0);
        assert_eq!(normalized.zoom.diameter, 64);
        assert_eq!(
            normalized.zoom.destination_offset,
            CoordinateOffset::new(2048, -2048)
        );
        assert_eq!(normalized.zoom.outline_thickness, 8);

        let minimums = CoordinateToolPreferences {
            halo: HaloPreferences {
                radius: i32::MIN,
                inversion_strength: -10.0,
                outline_thickness: i32::MAX,
                ..Default::default()
            },
            zoom: ZoomPreferences {
                zoom_factor: 100.0,
                diameter: i32::MAX,
                destination_offset: CoordinateOffset::new(-5000, 5000),
                outline_thickness: i32::MIN,
                ..Default::default()
            },
            ..Default::default()
        }
        .normalized();
        assert_eq!(minimums.halo.radius, 8);
        assert_eq!(minimums.halo.inversion_strength, 0.0);
        assert_eq!(minimums.halo.outline_thickness, 8);
        assert_eq!(minimums.zoom.zoom_factor, 4.0);
        assert_eq!(minimums.zoom.diameter, 480);
        assert_eq!(
            minimums.zoom.destination_offset,
            CoordinateOffset::new(-2048, 2048)
        );
        assert_eq!(minimums.zoom.outline_thickness, 1);

        let maximum_strength = HaloPreferences {
            inversion_strength: 10.0,
            ..Default::default()
        }
        .normalized();
        assert_eq!(maximum_strength.inversion_strength, 1.0);

        let below_range = ZoomPreferences {
            zoom_factor: 0.0,
            ..Default::default()
        }
        .normalized();
        assert_eq!(below_range.zoom_factor, 1.25);
        let non_finite_halo = HaloPreferences {
            inversion_strength: f32::NEG_INFINITY,
            ..Default::default()
        }
        .normalized();
        assert_eq!(non_finite_halo.inversion_strength, 0.4);
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
            halo: HaloPreferences {
                radius: 91,
                inversion_strength: 0.25,
                ..Default::default()
            },
            zoom: ZoomPreferences {
                mode: ZoomMode::Centered,
                destination_offset: CoordinateOffset::new(-130, 245),
                ..Default::default()
            },
            ..Default::default()
        };
        let value = serde_json::to_value(&preferences).unwrap();
        assert!(value.get("hud_enabled").is_none());
        assert!(value.get("crosshair_enabled").is_none());
        assert!(value.get("halo_enabled").is_none());
        assert!(value.get("zoom_enabled").is_none());
        assert_eq!(
            serde_json::from_value::<CoordinateToolPreferences>(value).unwrap(),
            preferences
        );
    }
}
