use crate::actions::Action;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub query: String,
    pub action: Action,
}

const HISTORY_FILE: &str = "history.json";

static HISTORY: Lazy<Mutex<VecDeque<HistoryEntry>>> = Lazy::new(|| {
    let hist = load_history_internal().unwrap_or_default();
    Mutex::new(hist)
});

fn load_history_internal() -> anyhow::Result<VecDeque<HistoryEntry>> {
    let content = std::fs::read_to_string(HISTORY_FILE).unwrap_or_default();
    if content.is_empty() {
        return Ok(VecDeque::new());
    }
    let list: Vec<HistoryEntry> = serde_json::from_str(&content)?;
    Ok(list.into())
}

/// Load history from `history.json` into the global HISTORY list.
pub fn load_history() -> anyhow::Result<()> {
    let hist = load_history_internal()?;
    let mut h = HISTORY.lock().unwrap();
    *h = hist;
    Ok(())
}

/// Save the current HISTORY list to `history.json`.
pub fn save_history() -> anyhow::Result<()> {
    let h = HISTORY.lock().unwrap();
    let list: Vec<HistoryEntry> = h.iter().cloned().collect();
    let json = serde_json::to_string_pretty(&list)?;
    std::fs::write(HISTORY_FILE, json)?;
    Ok(())
}

/// Append an entry to the history and persist the list.
pub fn append_history(entry: HistoryEntry) -> anyhow::Result<()> {
    {
        let mut h = HISTORY.lock().unwrap();
        h.push_front(entry);
        while h.len() > 100 {
            h.pop_back();
        }
    }
    save_history()
}

/// Return a clone of the current history list.
pub fn get_history() -> VecDeque<HistoryEntry> {
    HISTORY.lock().unwrap().clone()
}

