use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Action {
    pub label: String,
    pub desc: String,
    pub action: String, // Path to folder or exe
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

pub fn load_actions(path: &str) -> anyhow::Result<Vec<Action>> {
    let content = std::fs::read_to_string(path)?;
    let actions: Vec<Action> = serde_json::from_str(&content)?;
    Ok(actions)
}

pub fn save_actions(path: &str, actions: &[Action]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(actions)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub mod bookmarks;
pub mod calc;
pub mod clipboard;
pub mod exec;
pub mod fav;
pub mod folders;
pub mod history;
pub mod media;
pub mod rss;
pub mod screenshot;
pub mod shell;
pub mod snippets;
pub mod stopwatch;
pub mod system;
pub mod tempfiles;
pub mod timer;
pub mod todo;
