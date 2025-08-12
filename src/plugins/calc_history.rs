use serde::{Serialize, Deserialize};
use std::collections::VecDeque;

pub const CALC_HISTORY_FILE: &str = "calc_history.json";
/// Maximum number of entries kept in the calculator history.
pub const MAX_ENTRIES: usize = 20;

#[derive(Serialize, Deserialize, Clone)]
pub struct CalcHistoryEntry {
    pub expr: String,
    pub result: String,
}

/// Load calc history from `path`.
/// Returns empty queue when file missing or empty.
pub fn load_history(path: &str) -> anyhow::Result<VecDeque<CalcHistoryEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(VecDeque::new());
    }
    let list: Vec<CalcHistoryEntry> = serde_json::from_str(&content)?;
    Ok(list.into())
}

/// Save calc `history` to `path`.
pub fn save_history(path: &str, history: &VecDeque<CalcHistoryEntry>) -> anyhow::Result<()> {
    let list: Vec<CalcHistoryEntry> = history.iter().cloned().collect();
    let json = serde_json::to_string_pretty(&list)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Remove history entry at `index` from file at `path`.
pub fn remove_entry(path: &str, index: usize) -> anyhow::Result<()> {
    let mut history = load_history(path).unwrap_or_default();
    if index < history.len() {
        history.remove(index);
        save_history(path, &history)?;
    }
    Ok(())
}

/// Clear the calc history file at `path`.
pub fn clear_history_file(path: &str) -> anyhow::Result<()> {
    save_history(path, &VecDeque::new())
}

/// Append an entry to calc history at `path` keeping up to `max` items.
pub fn append_entry(path: &str, entry: CalcHistoryEntry, max: usize) -> anyhow::Result<()> {
    let mut history = load_history(path).unwrap_or_default();
    if let Some(pos) = history.iter().position(|e| e.expr == entry.expr && e.result == entry.result) {
        history.remove(pos);
    }
    history.push_front(entry);
    while history.len() > max {
        history.pop_back();
    }
    save_history(path, &history)
}

