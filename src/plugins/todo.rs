use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};

pub const TODO_FILE: &str = "todo.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct TodoEntry {
    pub text: String,
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
        if let Some(text) = query.strip_prefix("todo add ") {
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

        if let Some(pattern) = query.strip_prefix("todo rm ") {
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

        if let Some(rest) = query.strip_prefix("todo list") {
            let filter = rest.trim();
            let todos = load_todos(TODO_FILE).unwrap_or_default();
            return todos
                .into_iter()
                .enumerate()
                .filter(|(_, t)| self.matcher.fuzzy_match(&t.text, filter).is_some())
                .map(|(idx, t)| Action {
                    label: t.text,
                    desc: "Todo".into(),
                    action: format!("todo:item:{idx}"),
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
