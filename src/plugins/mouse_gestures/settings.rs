use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MouseGestureOverlaySettings {
    pub color: String,
    pub thickness: f32,
    pub fade: u64,
}

impl Default for MouseGestureOverlaySettings {
    fn default() -> Self {
        Self {
            color: "#ff66cc".to_string(),
            thickness: 2.0,
            fade: 300,
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
    #[serde(default)]
    pub preview_enabled: bool,
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
            preview_enabled: false,
        }
    }
}

fn default_match_threshold() -> f32 {
    0.7
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
