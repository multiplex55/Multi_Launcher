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
}

const HISTORY_FILE: &str = "history.json";

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
