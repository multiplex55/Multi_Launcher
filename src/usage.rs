use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UsageEntry {
    pub action: String,
    pub count: u32,
}

pub const USAGE_FILE: &str = "usage.json";

/// Load usage data from `path`.
///
/// Returns a map from action identifier to usage count.
pub fn load_usage(path: &str) -> anyhow::Result<HashMap<String, u32>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(HashMap::new());
    }
    let list: Vec<UsageEntry> = serde_json::from_str(&content)?;
    Ok(list.into_iter().map(|e| (e.action, e.count)).collect())
}

/// Save usage data in `usage` to `path`.
pub fn save_usage(path: &str, usage: &HashMap<String, u32>) -> anyhow::Result<()> {
    let list: Vec<UsageEntry> = usage
        .iter()
        .map(|(action, count)| UsageEntry {
            action: action.clone(),
            count: *count,
        })
        .collect();
    let json = serde_json::to_string_pretty(&list)?;
    std::fs::write(path, json)?;
    Ok(())
}
