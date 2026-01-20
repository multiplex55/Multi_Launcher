use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseGestureOverlaySettings {
    pub color: String,
    pub thickness: f32,
    pub fade: u64,
    #[serde(default)]
    pub max_render_points: usize,
}

impl Default for MouseGestureOverlaySettings {
    fn default() -> Self {
        Self {
            color: "#ff66cc".to_string(),
            thickness: 2.0,
            fade: 300,
            max_render_points: 512,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MouseGesturePluginSettings {
    pub enabled: bool,
    pub trigger_button: String,
    pub min_track_len: f32,
    #[serde(default)]
    pub min_point_distance: f32,
    pub max_distance: f32,
    #[serde(default = "default_match_threshold")]
    pub match_threshold: f32,
    #[serde(default)]
    pub max_track_len: f32,
    pub overlay: MouseGestureOverlaySettings,
    #[serde(default)]
    pub passthrough_on_no_match: bool,
    pub no_match_action: String,
    pub smoothing_enabled: bool,
    pub sampling_enabled: bool,
    #[serde(default = "default_sample_interval_ms")]
    pub sample_interval_ms: u64,
    #[serde(default = "default_segment_threshold_px")]
    pub segment_threshold_px: f32,
    #[serde(default = "default_direction_tolerance_deg")]
    pub direction_tolerance_deg: f32,
    #[serde(default = "default_straightness_threshold")]
    pub straightness_threshold: f32,
    #[serde(default = "default_straightness_min_displacement_px")]
    pub straightness_min_displacement_px: f32,
    #[serde(default = "default_tap_threshold_px")]
    pub tap_threshold_px: f32,
    #[serde(default)]
    pub preview_enabled: bool,
    #[serde(default)]
    pub preview_on_end_only: bool,
    #[serde(default)]
    pub debug_show_similarity: bool,
}

impl Default for MouseGesturePluginSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            trigger_button: "right".to_string(),
            min_track_len: 40.0,
            min_point_distance: 6.0,
            max_distance: 24.0,
            match_threshold: default_match_threshold(),
            max_track_len: 0.0,
            overlay: MouseGestureOverlaySettings::default(),
            passthrough_on_no_match: false,
            no_match_action: "none".to_string(),
            smoothing_enabled: true,
            sampling_enabled: true,
            sample_interval_ms: default_sample_interval_ms(),
            segment_threshold_px: default_segment_threshold_px(),
            direction_tolerance_deg: default_direction_tolerance_deg(),
            straightness_threshold: default_straightness_threshold(),
            straightness_min_displacement_px: default_straightness_min_displacement_px(),
            tap_threshold_px: default_tap_threshold_px(),
            preview_enabled: false,
            preview_on_end_only: false,
            debug_show_similarity: false,
        }
    }
}

impl MouseGesturePluginSettings {
    pub fn sample_interval_ms(&self) -> u64 {
        clamp_sample_interval_ms(self.sample_interval_ms)
    }
}

fn default_match_threshold() -> f32 {
    0.7
}

fn default_sample_interval_ms() -> u64 {
    16
}

fn default_segment_threshold_px() -> f32 {
    8.0
}

fn default_direction_tolerance_deg() -> f32 {
    30.0
}

fn default_straightness_threshold() -> f32 {
    0.9
}

fn default_straightness_min_displacement_px() -> f32 {
    80.0
}

fn default_tap_threshold_px() -> f32 {
    8.0
}

fn clamp_sample_interval_ms(value: u64) -> u64 {
    value.clamp(5, 50)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_settings_round_trip() {
        let settings = MouseGestureOverlaySettings {
            color: "#112233".into(),
            thickness: 3.5,
            fade: 450,
            max_render_points: 256,
        };
        let value = serde_json::to_value(&settings).expect("serialize overlay settings");
        let parsed: MouseGestureOverlaySettings =
            serde_json::from_value(value).expect("deserialize overlay settings");
        assert_eq!(parsed, settings);
    }

    #[test]
    fn plugin_settings_serializes_passthrough_flag() {
        let settings = MouseGesturePluginSettings {
            passthrough_on_no_match: true,
            ..Default::default()
        };
        let value = serde_json::to_value(&settings).expect("serialize plugin settings");
        let parsed: MouseGesturePluginSettings =
            serde_json::from_value(value).expect("deserialize plugin settings");
        assert!(parsed.passthrough_on_no_match);
    }
}
