use crate::common::persistence::{
    LoadState, PersistenceError, load_json, save_json_atomic_replaceable,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

pub const CALC_HISTORY_FILE: &str = "calc_history.json";
/// Maximum number of entries kept in the calculator history.
pub const MAX_ENTRIES: usize = 20;
static CALC_HISTORY_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct CalcHistoryEntry {
    pub expr: String,
    pub result: String,
}

/// Load calc history from `path`.
/// Returns empty queue when file missing or empty.
pub fn load_history(path: &str) -> anyhow::Result<VecDeque<CalcHistoryEntry>> {
    Ok(match load_history_typed(path)? {
        LoadState::Missing | LoadState::Empty => VecDeque::new(),
        LoadState::Loaded(list) => list.into(),
    })
}

pub fn load_history_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<CalcHistoryEntry>>, PersistenceError> {
    load_json(path)
}

/// Save calc `history` to `path`.
pub fn save_history(path: &str, history: &VecDeque<CalcHistoryEntry>) -> anyhow::Result<()> {
    let replacement = history.clone();
    update_history(path, move |current| {
        *current = replacement;
        Ok(true)
    })
    .map(|_| ())
}

/// Remove history entry at `index` from file at `path`.
pub fn remove_entry(path: &str, index: usize) -> anyhow::Result<()> {
    update_history(path, |history| {
        Ok((index < history.len())
            .then(|| history.remove(index))
            .is_some())
    })
    .map(|_| ())
}

/// Clear the calc history file at `path`.
pub fn clear_history_file(path: &str) -> anyhow::Result<()> {
    update_history(path, |history| {
        history.clear();
        Ok(true)
    })
    .map(|_| ())
}

/// Append an entry to calc history at `path` keeping up to `max` items.
pub fn append_entry(path: &str, entry: CalcHistoryEntry, max: usize) -> anyhow::Result<()> {
    update_history(path, move |history| {
        if let Some(pos) = history
            .iter()
            .position(|e| e.expr == entry.expr && e.result == entry.result)
        {
            history.remove(pos);
        }
        history.push_front(entry);
        while history.len() > max {
            history.pop_back();
        }
        Ok(true)
    })
    .map(|_| ())
}

fn update_history(
    path: &str,
    mutate: impl FnOnce(&mut VecDeque<CalcHistoryEntry>) -> anyhow::Result<bool>,
) -> anyhow::Result<VecDeque<CalcHistoryEntry>> {
    let _transaction = transaction_guard();
    let mut history = load_history(path)?;
    if mutate(&mut history)? {
        let list: Vec<_> = history.iter().cloned().collect();
        save_json_atomic_replaceable(path, &list)?;
    }
    Ok(history)
}

fn transaction_guard() -> MutexGuard<'static, ()> {
    CALC_HISTORY_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(expr: &str) -> CalcHistoryEntry {
        CalcHistoryEntry {
            expr: expr.into(),
            result: "1".into(),
        }
    }

    #[test]
    fn missing_initializes_but_malformed_rejects_mutations_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("calc.json");
        append_entry(path.to_str().unwrap(), entry("missing"), 20).unwrap();
        assert_eq!(load_history(path.to_str().unwrap()).unwrap().len(), 1);

        let invalid = b"not calculator history";
        std::fs::write(&path, invalid).unwrap();
        assert!(append_entry(path.to_str().unwrap(), entry("append"), 20).is_err());
        assert!(remove_entry(path.to_str().unwrap(), 0).is_err());
        assert!(clear_history_file(path.to_str().unwrap()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);

        assert!(append_entry(dir.path().to_str().unwrap(), entry("unreadable"), 20).is_err());
    }
}
