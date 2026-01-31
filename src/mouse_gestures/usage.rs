use crate::mouse_gestures::engine::DirMode;
use serde::{Deserialize, Serialize};

pub const GESTURES_USAGE_FILE: &str = "mouse_gestures_usage.json";
const MAX_USAGE_ENTRIES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GestureUsageEntry {
    pub timestamp: i64,
    pub gesture_label: String,
    pub tokens: String,
    pub dir_mode: DirMode,
    pub binding_idx: usize,
}

pub fn load_usage(path: &str) -> Vec<GestureUsageEntry> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Vec::new();
    }
    serde_json::from_str(&content).unwrap_or_default()
}

pub fn record_usage(path: &str, entry: GestureUsageEntry) {
    let mut usage = load_usage(path);
    usage.push(entry);
    if usage.len() > MAX_USAGE_ENTRIES {
        let drain = usage.len().saturating_sub(MAX_USAGE_ENTRIES);
        usage.drain(0..drain);
    }
    match serde_json::to_string_pretty(&usage) {
        Ok(json) => {
            if let Err(err) = std::fs::write(path, json) {
                tracing::error!(?err, "failed to save mouse gesture usage log");
            }
        }
        Err(err) => tracing::error!(?err, "failed to serialize mouse gesture usage log"),
    }
}
