use crate::actions::Action;
use crate::plugin::Plugin;
use std::collections::VecDeque;

pub const CLIPBOARD_FILE: &str = "clipboard_history.json";

pub fn load_history(path: &str) -> anyhow::Result<VecDeque<String>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(VecDeque::new());
    }
    let list: Vec<String> = serde_json::from_str(&content)?;
    Ok(list.into())
}

pub fn save_history(path: &str, history: &VecDeque<String>) -> anyhow::Result<()> {
    let list: Vec<String> = history.iter().cloned().collect();
    let json = serde_json::to_string_pretty(&list)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn remove_entry(path: &str, index: usize) -> anyhow::Result<()> {
    let mut history = load_history(path).unwrap_or_default();
    if index < history.len() {
        history.remove(index);
        save_history(path, &history)?;
    }
    Ok(())
}

pub fn set_entry(path: &str, index: usize, text: &str) -> anyhow::Result<()> {
    let mut history = load_history(path).unwrap_or_default();
    if index < history.len() {
        history[index] = text.to_string();
        save_history(path, &history)?;
    }
    Ok(())
}

pub fn clear_history_file(path: &str) -> anyhow::Result<()> {
    save_history(path, &VecDeque::new())
}

pub struct ClipboardPlugin {
    max_entries: usize,
    path: String,
}

impl ClipboardPlugin {
    pub fn new(max_entries: usize) -> Self {
        Self { max_entries, path: CLIPBOARD_FILE.into() }
    }

    fn update_history(&self) -> VecDeque<String> {
        let mut history = load_history(&self.path).unwrap_or_default();
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(txt) = clipboard.get_text() {
                if history.front().map(|v| v != &txt).unwrap_or(true) {
                    if let Some(pos) = history.iter().position(|v| v == &txt) {
                        history.remove(pos);
                    }
                    history.push_front(txt.clone());
                    while history.len() > self.max_entries {
                        history.pop_back();
                    }
                    let _ = save_history(&self.path, &history);
                }
            }
        }
        history
    }
}

impl Default for ClipboardPlugin {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Plugin for ClipboardPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.starts_with("cb") {
            return Vec::new();
        }

        let trimmed = query.trim();
        if trimmed == "cb" {
            return vec![Action {
                label: "cb: edit clipboard".into(),
                desc: "Clipboard".into(),
                action: "clipboard:dialog".into(),
                args: None,
            }];
        }

        if trimmed == "cb clear" {
            return vec![Action {
                label: "Clear clipboard history".into(),
                desc: "Clipboard".into(),
                action: "clipboard:clear".into(),
                args: None,
            }];
        }

        let filter = query.strip_prefix("cb").unwrap_or("").trim();
        let history = self.update_history();
        history
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.contains(filter))
            .map(|(idx, entry)| Action {
                label: entry.clone(),
                desc: "Clipboard".into(),
                action: format!("clipboard:copy:{idx}"),
                args: None,
            })
            .collect()
    }

    fn name(&self) -> &str {
        "clipboard"
    }

    fn description(&self) -> &str {
        "Provides clipboard history search (prefix: `cb`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}
