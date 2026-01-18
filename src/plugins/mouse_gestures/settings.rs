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
    pub max_distance: f32,
    pub overlay: MouseGestureOverlaySettings,
    pub no_match_action: String,
    pub smoothing_enabled: bool,
    pub sampling_enabled: bool,
}

impl Default for MouseGesturePluginSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            trigger_button: "right".to_string(),
            min_track_len: 40.0,
            max_distance: 24.0,
            overlay: MouseGestureOverlaySettings::default(),
            no_match_action: "none".to_string(),
            smoothing_enabled: true,
            sampling_enabled: true,
        }
    }
}
