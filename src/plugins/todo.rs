use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};

pub const TODO_FILE: &str = "todo.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct TodoEntry {
    pub text: String,
    pub done: bool,
}

pub fn load_todos(path: &str) -> anyhow::Result<Vec<TodoEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<TodoEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

pub fn save_todos(path: &str, todos: &[TodoEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(todos)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn append_todo(path: &str, text: &str) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    list.push(TodoEntry {
        text: text.to_string(),
        done: false,
    });
    save_todos(path, &list)
}

pub fn remove_todo(path: &str, index: usize) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    if index < list.len() {
        list.remove(index);
        save_todos(path, &list)?;
    }
    Ok(())
}

pub fn mark_done(path: &str, index: usize) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    if let Some(entry) = list.get_mut(index) {
        entry.done = true;
        save_todos(path, &list)?;
    }
    Ok(())
}

pub fn clear_done(path: &str) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    let orig_len = list.len();
    list.retain(|e| !e.done);
    if list.len() != orig_len {
        save_todos(path, &list)?;
    }
    Ok(())
}

pub struct TodoPlugin {
    matcher: SkimMatcherV2,
}

impl TodoPlugin {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
        }
    }
}

impl Default for TodoPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for TodoPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();

        if trimmed.eq("todo") {
            return vec![Action {
                label: "todo: edit todos".into(),
                desc: "Todo".into(),
                action: "todo:dialog".into(),
                args: None,
            }];
        }

        if trimmed.eq("todo clear") {
            return vec![Action {
                label: "Clear completed todos".into(),
                desc: "Todo".into(),
                action: "todo:clear".into(),
                args: None,
            }];
        }

        if let Some(text) = trimmed.strip_prefix("todo add ") {
            let text = text.trim();
            if !text.is_empty() {
                return vec![Action {
                    label: format!("Add todo {text}"),
                    desc: "Todo".into(),
                    action: format!("todo:add:{text}"),
                    args: None,
                }];
            }
        }

        if let Some(pattern) = trimmed.strip_prefix("todo rm ") {
            let filter = pattern.trim();
            let todos = load_todos(TODO_FILE).unwrap_or_default();
            return todos
                .into_iter()
                .enumerate()
                .filter(|(_, t)| self.matcher.fuzzy_match(&t.text, filter).is_some())
                .map(|(idx, t)| Action {
                    label: format!("Remove todo {}", t.text),
                    desc: "Todo".into(),
                    action: format!("todo:remove:{idx}"),
                    args: None,
                })
                .collect();
        }

        if let Some(rest) = trimmed.strip_prefix("todo list") {
            let filter = rest.trim();
            let todos = load_todos(TODO_FILE).unwrap_or_default();
            return todos
                .into_iter()
                .enumerate()
                .filter(|(_, t)| self.matcher.fuzzy_match(&t.text, filter).is_some())
                .map(|(idx, t)| Action {
                    label: format!("{} {}", if t.done { "[x]" } else { "[ ]" }, t.text),
                    desc: "Todo".into(),
                    action: format!("todo:done:{idx}"),
                    args: None,
                })
                .collect();
        }

        Vec::new()
    }

    fn name(&self) -> &str {
        "todo"
    }

    fn description(&self) -> &str {
        "Manage todo items (prefix: `todo`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}
