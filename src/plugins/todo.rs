use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const TODO_FILE: &str = "todo.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct TodoEntry {
    pub text: String,
    pub done: bool,
}

/// Load todo entries from `path`.
pub fn load_todos(path: &str) -> anyhow::Result<Vec<TodoEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<TodoEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

/// Save `todos` to `path` as JSON.
pub fn save_todos(path: &str, todos: &[TodoEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(todos)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Append a new todo entry with `text`.
pub fn append_todo(path: &str, text: &str) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    list.push(TodoEntry {
        text: text.to_string(),
        done: false,
    });
    save_todos(path, &list)
}

/// Remove the todo at `index` from the list stored at `path`.
pub fn remove_todo(path: &str, index: usize) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    if index < list.len() {
        list.remove(index);
        save_todos(path, &list)?;
    }
    Ok(())
}

/// Toggle completion status of the todo at `index` in `path`.
pub fn mark_done(path: &str, index: usize) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    if let Some(entry) = list.get_mut(index) {
        entry.done = !entry.done;
        save_todos(path, &list)?;
    }
    Ok(())
}

/// Remove all completed todos from `path`.
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
    data: Arc<Mutex<Vec<TodoEntry>>>,
    #[allow(dead_code)]
    watcher: Option<RecommendedWatcher>,
}

impl TodoPlugin {
    /// Create a new todo plugin with a fuzzy matcher.
    pub fn new() -> Self {
        let data = Arc::new(Mutex::new(load_todos(TODO_FILE).unwrap_or_default()));
        let data_clone = data.clone();
        let path = TODO_FILE.to_string();
        let mut watcher = RecommendedWatcher::new(
            {
                let path = path.clone();
                move |res: notify::Result<notify::Event>| {
                    if let Ok(event) = res {
                        if matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        ) {
                            if let Ok(list) = load_todos(&path) {
                                if let Ok(mut lock) = data_clone.lock() {
                                    *lock = list;
                                }
                            }
                        }
                    }
                }
            },
            Config::default(),
        )
        .ok();
        if let Some(w) = watcher.as_mut() {
            let p = std::path::Path::new(&path);
            if w.watch(p, RecursiveMode::NonRecursive).is_err() {
                let parent = p.parent().unwrap_or_else(|| std::path::Path::new("."));
                let _ = w.watch(parent, RecursiveMode::NonRecursive);
            }
        }
        Self {
            matcher: SkimMatcherV2::default(),
            data,
            watcher,
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

        if trimmed.eq_ignore_ascii_case("todo") {
            return vec![Action {
                label: "todo: edit todos".into(),
                desc: "Todo".into(),
                action: "todo:dialog".into(),
                args: None,
            }];
        }

        if trimmed.eq_ignore_ascii_case("todo clear") {
            return vec![Action {
                label: "Clear completed todos".into(),
                desc: "Todo".into(),
                action: "todo:clear".into(),
                args: None,
            }];
        }

        if trimmed.eq_ignore_ascii_case("todo add") {
            return vec![Action {
                label: "todo: edit todos".into(),
                desc: "Todo".into(),
                action: "todo:dialog".into(),
                args: None,
            }];
        }

        const ADD_PREFIX: &str = "todo add ";
        if trimmed.len() >= ADD_PREFIX.len()
            && trimmed[..ADD_PREFIX.len()].eq_ignore_ascii_case(ADD_PREFIX)
        {
            let text = trimmed[ADD_PREFIX.len()..].trim();
            if !text.is_empty() {
                return vec![Action {
                    label: format!("Add todo {text}"),
                    desc: "Todo".into(),
                    action: format!("todo:add:{text}"),
                    args: None,
                }];
            }
        }

        const RM_PREFIX: &str = "todo rm ";
        if trimmed.len() >= RM_PREFIX.len()
            && trimmed[..RM_PREFIX.len()].eq_ignore_ascii_case(RM_PREFIX)
        {
            let filter = trimmed[RM_PREFIX.len()..].trim();
            let todos = self.data.lock().unwrap().clone();
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

        const LIST_PREFIX: &str = "todo list";
        if trimmed.len() >= LIST_PREFIX.len()
            && trimmed[..LIST_PREFIX.len()].eq_ignore_ascii_case(LIST_PREFIX)
        {
            let filter = trimmed[LIST_PREFIX.len()..].trim();
            let todos = self.data.lock().unwrap().clone();
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

    fn commands(&self) -> Vec<Action> {
        vec![
            Action { label: "todo".into(), desc: "todo".into(), action: "fill:todo ".into(), args: None },
            Action { label: "todo add".into(), desc: "todo".into(), action: "fill:todo add ".into(), args: None },
            Action { label: "todo rm".into(), desc: "todo".into(), action: "fill:todo rm ".into(), args: None },
            Action { label: "todo list".into(), desc: "todo".into(), action: "fill:todo list".into(), args: None },
            Action { label: "todo clear".into(), desc: "todo".into(), action: "fill:todo clear".into(), args: None },
        ]
    }
}
