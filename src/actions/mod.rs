use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

static ACTIONS_VERSION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Action {
    pub label: String,
    pub desc: String,
    pub action: String, // Path to folder or exe
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<ActionRiskLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

pub fn load_actions(path: &str) -> anyhow::Result<Vec<Action>> {
    let content = std::fs::read_to_string(path)?;
    let actions: Vec<Action> = serde_json::from_str(&content)?;
    Ok(actions)
}

pub fn save_actions(path: &str, actions: &[Action]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(actions)?;
    std::fs::write(path, json)?;
    bump_actions_version();
    Ok(())
}

pub fn actions_version() -> u64 {
    ACTIONS_VERSION.load(Ordering::SeqCst)
}

pub fn bump_actions_version() {
    ACTIONS_VERSION.fetch_add(1, Ordering::SeqCst);
}

pub mod bookmarks;
pub mod calc;
pub mod clipboard;
pub mod exec;
pub mod fav;
pub mod folders;
pub mod history;
pub mod keys;
pub mod layout;
pub mod media;
pub mod screenshot;
pub mod shell;
pub mod snippets;
pub mod stopwatch;
pub mod system;
pub mod tempfiles;
pub mod timer;
pub mod todo;
