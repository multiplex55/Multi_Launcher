use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use crate::common::json_watch::{watch_json, JsonWatcher};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const TODO_FILE: &str = "todo.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct TodoEntry {
    pub text: String,
    pub done: bool,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Sort todo entries by priority descending (highest priority first).
pub fn sort_by_priority_desc(entries: &mut Vec<TodoEntry>) {
    entries.sort_by(|a, b| b.priority.cmp(&a.priority));
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

/// Append a new todo entry with `text`, `priority` and `tags`.
pub fn append_todo(path: &str, text: &str, priority: u8, tags: &[String]) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    list.push(TodoEntry {
        text: text.to_string(),
        done: false,
        priority,
        tags: tags.to_vec(),
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

/// Set the priority of the todo at `index` in `path`.
pub fn set_priority(path: &str, index: usize, priority: u8) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    if let Some(entry) = list.get_mut(index) {
        entry.priority = priority;
        save_todos(path, &list)?;
    }
    Ok(())
}

/// Replace the tags of the todo at `index` in `path`.
pub fn set_tags(path: &str, index: usize, tags: &[String]) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    if let Some(entry) = list.get_mut(index) {
        entry.tags = tags.to_vec();
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
    watcher: Option<JsonWatcher>,
}

impl TodoPlugin {
    /// Create a new todo plugin with a fuzzy matcher.
    pub fn new() -> Self {
        let data = Arc::new(Mutex::new(load_todos(TODO_FILE).unwrap_or_default()));
        let data_clone = data.clone();
        let path = TODO_FILE.to_string();
        let watch_path = path.clone();
        let watcher = watch_json(&watch_path, {
            let watch_path = watch_path.clone();
            move || {
                if let Ok(list) = load_todos(&watch_path) {
                    if let Ok(mut lock) = data_clone.lock() {
                        *lock = list;
                    }
                }
            }
        })
        .ok();
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

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "todo") {
            if rest.is_empty() {
                return vec![Action {
                    label: "todo: edit todos".into(),
                    desc: "Todo".into(),
                    action: "todo:dialog".into(),
                    args: None,
                }];
            }
        }

        const EDIT_PREFIX: &str = "todo edit";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, EDIT_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            let mut entries: Vec<(usize, &TodoEntry)> = guard.iter().enumerate().collect();

            let tag_filter = filter.starts_with('#');
            if tag_filter {
                let tag = filter.trim_start_matches('#');
                entries.retain(|(_, t)| t.tags.iter().any(|tg| tg.eq_ignore_ascii_case(tag)));
            } else if !filter.is_empty() {
                entries.retain(|(_, t)| self.matcher.fuzzy_match(&t.text, filter).is_some());
            }

            if filter.is_empty() || tag_filter {
                entries.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));
            }

            return entries
                .into_iter()
                .map(|(idx, t)| Action {
                    label: format!("{} {}", if t.done { "[x]" } else { "[ ]" }, t.text.clone()),
                    desc: "Todo".into(),
                    action: format!("todo:edit:{idx}"),
                    args: None,
                })
                .collect();
        }

        if trimmed.eq_ignore_ascii_case("todo view") {
            return vec![Action {
                label: "todo: view list".into(),
                desc: "Todo".into(),
                action: "todo:view".into(),
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
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, ADD_PREFIX) {
            let rest = rest.trim();
            if !rest.is_empty() {
                let mut priority: u8 = 0;
                let mut tags: Vec<String> = Vec::new();
                let mut words: Vec<String> = Vec::new();
                for part in rest.split_whitespace() {
                    if let Some(p) = part.strip_prefix("p=") {
                        if let Ok(n) = p.parse::<u8>() {
                            priority = n;
                        }
                    } else if let Some(tag) = part.strip_prefix('#') {
                        if !tag.is_empty() {
                            tags.push(tag.to_string());
                        }
                    } else {
                        words.push(part.to_string());
                    }
                }
                let text = words.join(" ");
                if !text.is_empty() {
                    let tag_str = tags.join(",");
                    return vec![Action {
                        label: format!("Add todo {text}"),
                        desc: "Todo".into(),
                        action: format!("todo:add:{text}|{priority}|{tag_str}"),
                        args: None,
                    }];
                }
            }
        }

        const PSET_PREFIX: &str = "todo pset ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, PSET_PREFIX) {
            let rest = rest.trim();
            let mut parts = rest.split_whitespace();
            if let (Some(idx_str), Some(priority_str)) = (parts.next(), parts.next()) {
                if let (Ok(idx), Ok(priority)) =
                    (idx_str.parse::<usize>(), priority_str.parse::<u8>())
                {
                    return vec![Action {
                        label: format!("Set priority {priority} for todo {idx}"),
                        desc: "Todo".into(),
                        action: format!("todo:pset:{idx}|{priority}"),
                        args: None,
                    }];
                }
            }
        }

        const TAG_PREFIX: &str = "todo tag ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, TAG_PREFIX) {
            let rest = rest.trim();
            let mut parts = rest.split_whitespace();
            if let Some(idx_str) = parts.next() {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    let mut tags: Vec<String> = Vec::new();
                    for t in parts {
                        if let Some(tag) = t.strip_prefix('#') {
                            if !tag.is_empty() {
                                tags.push(tag.to_string());
                            }
                        }
                    }
                    let tag_str = tags.join(",");
                    return vec![Action {
                        label: format!("Set tags for todo {idx}"),
                        desc: "Todo".into(),
                        action: format!("todo:tag:{idx}|{tag_str}"),
                        args: None,
                    }];
                }
            }
        }

        const RM_PREFIX: &str = "todo rm ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, RM_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .enumerate()
                .filter(|(_, t)| self.matcher.fuzzy_match(&t.text, filter).is_some())
                .map(|(idx, t)| Action {
                    label: format!("Remove todo {}", t.text.clone()),
                    desc: "Todo".into(),
                    action: format!("todo:remove:{idx}"),
                    args: None,
                })
                .collect();
        }

        const LIST_PREFIX: &str = "todo list";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, LIST_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            let mut entries: Vec<(usize, &TodoEntry)> = guard.iter().enumerate().collect();

            let tag_filter = filter.starts_with('#');
            if tag_filter {
                let tag = filter.trim_start_matches('#');
                entries.retain(|(_, t)| t.tags.iter().any(|tg| tg.eq_ignore_ascii_case(tag)));
            } else if !filter.is_empty() {
                entries.retain(|(_, t)| self.matcher.fuzzy_match(&t.text, filter).is_some());
            }

            if filter.is_empty() || tag_filter {
                entries.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));
            }

            return entries
                .into_iter()
                .map(|(idx, t)| Action {
                    label: format!("{} {}", if t.done { "[x]" } else { "[ ]" }, t.text.clone()),
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
            Action {
                label: "todo".into(),
                desc: "Todo".into(),
                action: "query:todo".into(),
                args: None,
            },
            Action {
                label: "todo add".into(),
                desc: "Todo".into(),
                action: "query:todo add ".into(),
                args: None,
            },
            Action {
                label: "todo list".into(),
                desc: "Todo".into(),
                action: "query:todo list".into(),
                args: None,
            },
            Action {
                label: "todo rm".into(),
                desc: "Todo".into(),
                action: "query:todo rm ".into(),
                args: None,
            },
            Action {
                label: "todo clear".into(),
                desc: "Todo".into(),
                action: "query:todo clear".into(),
                args: None,
            },
            Action {
                label: "todo pset".into(),
                desc: "Todo".into(),
                action: "query:todo pset ".into(),
                args: None,
            },
            Action {
                label: "todo tag".into(),
                desc: "Todo".into(),
                action: "query:todo tag ".into(),
                args: None,
            },
            Action {
                label: "todo edit".into(),
                desc: "Todo".into(),
                action: "query:todo edit".into(),
                args: None,
            },
            Action {
                label: "todo view".into(),
                desc: "Todo".into(),
                action: "query:todo view ".into(),
                args: None,
            },
        ]
    }
}
