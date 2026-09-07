use crate::common::persistence::{
    LoadState, PersistenceError, load_json, save_json_atomic_replaceable,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UsageEntry {
    pub action: String,
    pub count: u32,
}

pub const USAGE_FILE: &str = "usage.json";

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
}

pub fn load_usage_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<UsageEntry>>, PersistenceError> {
    load_json(path)
}

/// Save usage data in `usage` to `path`.
pub fn save_usage(path: &str, usage: &HashMap<String, u32>) -> anyhow::Result<()> {
    // Revalidate the existing replaceable store so a session using temporary
    // empty state cannot overwrite malformed or unreadable bytes on exit.
    let _ = load_usage_typed(path)?;
    let list: Vec<UsageEntry> = usage
        .iter()
        .map(|(action, count)| UsageEntry {
            action: action.clone(),
            count: *count,
        })
        .collect();
    save_json_atomic_replaceable(path, &list).map_err(Into::into)
}
