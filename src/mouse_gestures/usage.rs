use crate::common::persistence::{LoadState, load_json, save_json_atomic_replaceable};
use crate::mouse_gestures::engine::DirMode;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub const GESTURES_USAGE_FILE: &str = "mouse_gestures_usage.json";
const MAX_USAGE_ENTRIES: usize = 100;
static USAGE_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GestureUsageEntry {
    pub timestamp: i64,
    pub gesture_label: String,
    pub tokens: String,
    pub dir_mode: DirMode,
    pub binding_idx: usize,
}

pub fn load_usage(path: &str) -> Vec<GestureUsageEntry> {
    load_usage_strict(path).unwrap_or_else(|error| {
        tracing::error!(%error, "mouse gesture usage is invalid; retaining a temporary empty view");
        Vec::new()
    })
}

fn load_usage_strict(path: &str) -> anyhow::Result<Vec<GestureUsageEntry>> {
    Ok(match load_json(path)? {
        LoadState::Missing | LoadState::Empty => Vec::new(),
        LoadState::Loaded(usage) => usage,
    })
}

pub fn record_usage(path: &str, entry: GestureUsageEntry) {
    let _transaction = USAGE_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut usage = match load_usage_strict(path) {
        Ok(usage) => usage,
        Err(error) => {
            tracing::error!(%error, "mouse gesture usage update skipped");
            return;
        }
    };
    usage.push(entry);
    if usage.len() > MAX_USAGE_ENTRIES {
        let drain = usage.len().saturating_sub(MAX_USAGE_ENTRIES);
        usage.drain(0..drain);
    }
    if let Err(err) = save_json_atomic_replaceable(path, &usage) {
        tracing::error!(?err, "failed to save mouse gesture usage log");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> GestureUsageEntry {
        GestureUsageEntry {
            timestamp: 1,
            gesture_label: "Open".into(),
            tokens: "R".into(),
            dir_mode: DirMode::Four,
            binding_idx: 0,
        }
    }

    #[test]
    fn missing_initializes_but_malformed_record_is_skipped_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gesture-usage.json");
        let path_str = path.to_str().unwrap();
        record_usage(path_str, entry());
        assert_eq!(load_usage(path_str), vec![entry()]);

        let invalid = b"not gesture usage";
        std::fs::write(&path, invalid).unwrap();
        record_usage(path_str, entry());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }
}
