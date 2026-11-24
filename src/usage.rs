use crate::common::persistence::{
    LoadState, PersistenceError, load_json, save_json_atomic_replaceable,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UsageEntry {
    pub action: String,
    pub count: u32,
}

pub const USAGE_FILE: &str = "usage.json";

static USAGE_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub(crate) fn transaction_guard() -> MutexGuard<'static, ()> {
    USAGE_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Load usage data from `path`.
///
/// Returns a map from action identifier to usage count.
pub fn load_usage(path: &str) -> anyhow::Result<HashMap<String, u32>> {
    let list = match load_usage_typed(path)? {
        LoadState::Missing | LoadState::Empty => Vec::new(),
        LoadState::Loaded(list) => list,
    };
    Ok(list.into_iter().map(|e| (e.action, e.count)).collect())
}

/// Increment one action in the canonical store and return its saved count.
pub fn record_usage(path: &str, action: &str) -> anyhow::Result<u32> {
    let _permit = crate::common::repair_watch::require_path_read(Path::new(path))?;
    let _transaction = transaction_guard();
    let mut list = match load_usage_typed_unfenced(path)? {
        LoadState::Missing | LoadState::Empty => Vec::new(),
        LoadState::Loaded(list) => list,
    };
    let count = if let Some(entry) = list.iter_mut().rev().find(|entry| entry.action == action) {
        entry.count = entry.count.saturating_add(1);
        entry.count
    } else {
        list.push(UsageEntry {
            action: action.to_owned(),
            count: 1,
        });
        1
    };
    save_json_atomic_replaceable(path, &list)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_initializes_but_malformed_is_not_overwritten_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        let path_str = path.to_str().unwrap();
        let usage = HashMap::from([("action:test".to_string(), 2)]);
        save_usage(path_str, &usage).unwrap();
        assert_eq!(load_usage(path_str).unwrap(), usage);

        let invalid = b"not usage";
        std::fs::write(&path, invalid).unwrap();
        assert!(save_usage(path_str, &HashMap::new()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }

    #[test]
    fn record_usage_preserves_latest_unrelated_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        let path_str = path.to_str().unwrap();
        save_usage(path_str, &HashMap::from([("other".to_owned(), 7)])).unwrap();
        assert_eq!(record_usage(path_str, "target").unwrap(), 1);
        assert_eq!(record_usage(path_str, "other").unwrap(), 8);
        assert_eq!(
            load_usage(path_str).unwrap(),
            HashMap::from([("target".to_owned(), 1), ("other".to_owned(), 8),])
        );
    }

    #[test]
    fn record_usage_updates_last_duplicate_and_saturates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        let path_str = path.to_str().unwrap();
        let mut entries = vec![
            UsageEntry {
                action: "target".into(),
                count: 3,
            },
            UsageEntry {
                action: "other".into(),
                count: 6,
            },
            UsageEntry {
                action: "target".into(),
                count: u32::MAX - 1,
            },
        ];
        save_json_atomic_replaceable(&path, &entries).unwrap();
        assert_eq!(record_usage(path_str, "target").unwrap(), u32::MAX);
        entries[2].count = u32::MAX;
        assert_eq!(record_usage(path_str, "target").unwrap(), u32::MAX);
        assert_eq!(
            load_usage_typed(path_str).unwrap(),
            LoadState::Loaded(entries)
        );
    }

    #[test]
    fn record_usage_refuses_malformed_existing_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        let invalid = b"not usage";
        std::fs::write(&path, invalid).unwrap();
        assert!(record_usage(path.to_str().unwrap(), "target").is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }
}

pub fn load_usage_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<UsageEntry>>, PersistenceError> {
    let path = path.as_ref();
    let _permit = crate::common::repair_watch::require_persisted_read(path)?;
    load_usage_typed_unfenced(path)
}

fn load_usage_typed_unfenced(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<UsageEntry>>, PersistenceError> {
    load_json(path)
}

/// Save usage data in `usage` to `path`.
pub fn save_usage(path: &str, usage: &HashMap<String, u32>) -> anyhow::Result<()> {
    let _permit = crate::common::repair_watch::require_path_read(Path::new(path))?;
    let _transaction = transaction_guard();
    save_usage_unfenced(path, usage)
}

/// Save usage while the caller already owns the usage transaction lock.
pub(crate) fn save_usage_locked(path: &str, usage: &HashMap<String, u32>) -> anyhow::Result<()> {
    save_usage_unfenced(path, usage)
}

fn save_usage_unfenced(path: &str, usage: &HashMap<String, u32>) -> anyhow::Result<()> {
    // Revalidate the existing replaceable store so a session using temporary
    // empty state cannot overwrite malformed or unreadable bytes on exit.
    let _ = load_usage_typed_unfenced(path)?;
    let list: Vec<UsageEntry> = usage
        .iter()
        .map(|(action, count)| UsageEntry {
            action: action.clone(),
            count: *count,
        })
        .collect();
    save_json_atomic_replaceable(path, &list)?;
    Ok(())
}
