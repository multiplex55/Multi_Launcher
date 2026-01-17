use crate::actions::Action;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::RwLock;

#[derive(Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub query: String,
    #[serde(skip)]
    pub query_lc: String,
    pub action: Action,
    #[serde(default)]
    pub timestamp: i64,
}

const HISTORY_FILE: &str = "history.json";
pub const HISTORY_PINS_FILE: &str = "history_pins.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct HistoryPin {
    pub action_id: String,
    pub label: String,
    pub desc: String,
    pub args: Option<String>,
    pub query: String,
    #[serde(default)]
    pub timestamp: i64,
}

impl HistoryPin {
    pub fn from_history(entry: &HistoryEntry) -> Self {
        Self {
            action_id: entry.action.action.clone(),
            label: entry.action.label.clone(),
            desc: entry.action.desc.clone(),
            args: entry.action.args.clone(),
            query: entry.query.clone(),
            timestamp: entry.timestamp,
        }
    }
}

impl PartialEq for HistoryPin {
    fn eq(&self, other: &Self) -> bool {
        self.action_id == other.action_id
            && self.query == other.query
            && self.timestamp == other.timestamp
    }
}

impl Eq for HistoryPin {}

static HISTORY: Lazy<RwLock<VecDeque<HistoryEntry>>> = Lazy::new(|| {
    let hist = load_history_internal().unwrap_or_else(|e| {
        tracing::error!("failed to load history: {e}");
        VecDeque::new()
    });
    RwLock::new(hist)
});

pub fn poison_history_lock() {
    let _ = std::panic::catch_unwind(|| {
        if let Ok(_guard) = HISTORY.write() {
            panic!("poison");
        }
    });
}

fn load_history_internal() -> anyhow::Result<VecDeque<HistoryEntry>> {
    let content = std::fs::read_to_string(HISTORY_FILE).unwrap_or_default();
    if content.is_empty() {
        return Ok(VecDeque::new());
    }
    let mut list: Vec<HistoryEntry> = serde_json::from_str(&content)?;
    for e in &mut list {
        e.query_lc = e.query.to_lowercase();
    }
    Ok(list.into())
}

/// Save the current HISTORY list to `history.json`.
pub fn save_history() -> anyhow::Result<()> {
    let Some(h) = HISTORY.read().ok() else {
        return Ok(());
    };
    let list: Vec<HistoryEntry> = h.iter().cloned().collect();
    let json = serde_json::to_string_pretty(&list)?;
    std::fs::write(HISTORY_FILE, json)?;
    Ok(())
}

/// Append an entry to the history and persist the list. The `limit` parameter
/// specifies the maximum number of entries kept.
pub fn append_history(mut entry: HistoryEntry, limit: usize) -> anyhow::Result<()> {
    entry.query_lc = entry.query.to_lowercase();
    if entry.timestamp == 0 {
        entry.timestamp = chrono::Utc::now().timestamp();
    }
    {
        let Some(mut h) = HISTORY.write().ok() else {
            return Ok(());
        };
        h.push_front(entry);
        while h.len() > limit {
            h.pop_back();
        }
    }
    save_history()
}

/// Run a closure while holding a lock on the history list.
///
/// The closure receives a reference to the current list which should only be
/// used within the scope of the closure. This avoids cloning the entire
/// history for read-only operations.
pub fn with_history<R>(f: impl FnOnce(&VecDeque<HistoryEntry>) -> R) -> Option<R> {
    let h = HISTORY.read().ok()?;
    Some(f(&h))
}

/// Return a clone of the current history list.
pub fn get_history() -> VecDeque<HistoryEntry> {
    with_history(|h| h.iter().cloned().collect()).unwrap_or_default()
}

/// Clear all history entries and persist the empty list to `history.json`.
pub fn clear_history() -> anyhow::Result<()> {
    {
        let Some(mut h) = HISTORY.write().ok() else {
            return Ok(());
        };
        h.clear();
    }
    save_history()
}

pub fn load_pins(path: &str) -> anyhow::Result<Vec<HistoryPin>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<HistoryPin> = serde_json::from_str(&content)?;
    Ok(list)
}

pub fn save_pins(path: &str, pins: &[HistoryPin]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(pins)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn toggle_pin(path: &str, pin: &HistoryPin) -> anyhow::Result<bool> {
    let mut pins = load_pins(path).unwrap_or_default();
    if let Some(idx) = pins.iter().position(|p| p == pin) {
        pins.remove(idx);
        save_pins(path, &pins)?;
        Ok(false)
    } else {
        pins.push(pin.clone());
        save_pins(path, &pins)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::{load_pins, save_pins, toggle_pin, HistoryPin};
    use tempfile::tempdir;

    #[test]
    fn pin_roundtrip_and_toggle() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("pins.json");
        let pin = HistoryPin {
            action_id: "action:one".into(),
            label: "One".into(),
            desc: "Test".into(),
            args: Some("--flag".into()),
            query: "one".into(),
            timestamp: 123,
        };

        save_pins(path.to_str().unwrap(), &[pin.clone()]).expect("save pins");
        let loaded = load_pins(path.to_str().unwrap()).expect("load pins");
        assert_eq!(loaded, vec![pin.clone()]);

        let now_pinned = toggle_pin(path.to_str().unwrap(), &pin).expect("toggle off");
        assert!(!now_pinned);
        let cleared = load_pins(path.to_str().unwrap()).expect("load after clear");
        assert!(cleared.is_empty());

        let now_pinned = toggle_pin(path.to_str().unwrap(), &pin).expect("toggle on");
        assert!(now_pinned);
        let reloaded = load_pins(path.to_str().unwrap()).expect("load after add");
        assert_eq!(reloaded, vec![pin]);
    }
}
