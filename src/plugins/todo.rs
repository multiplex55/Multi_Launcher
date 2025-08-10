//! Todo plugin and helpers.
//!
//! `TODO_DATA` is a process-wide cache of todos loaded from `todo.json`.
//! Any operation that writes to disk updates this cache, and a `JsonWatcher`
//! refreshes it when the file changes externally. This keeps plugin state and
//! tests synchronized with the latest on-disk data.

use crate::actions::Action;
use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use once_cell::sync::Lazy;
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

/// Shared in-memory todo cache kept in sync with `todo.json`.
/// Disk writes and the [`JsonWatcher`] ensure updates are visible immediately
/// to all plugin instances and tests.
pub static TODO_DATA: Lazy<Arc<Mutex<Vec<TodoEntry>>>> =
    Lazy::new(|| Arc::new(Mutex::new(load_todos(TODO_FILE).unwrap_or_default())));

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

fn update_cache(list: Vec<TodoEntry>) {
    if let Ok(mut lock) = TODO_DATA.lock() {
        *lock = list;
    }
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
    save_todos(path, &list)?;
    update_cache(list);
    Ok(())
}

/// Remove the todo at `index` from the list stored at `path`.
pub fn remove_todo(path: &str, index: usize) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    if index < list.len() {
        list.remove(index);
        save_todos(path, &list)?;
        update_cache(list);
    }
    Ok(())
}

/// Toggle completion status of the todo at `index` in `path`.
pub fn mark_done(path: &str, index: usize) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    if let Some(entry) = list.get_mut(index) {
        entry.done = !entry.done;
        save_todos(path, &list)?;
        update_cache(list);
    }
    Ok(())
}

/// Set the priority of the todo at `index` in `path`.
pub fn set_priority(path: &str, index: usize, priority: u8) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    if let Some(entry) = list.get_mut(index) {
        entry.priority = priority;
        save_todos(path, &list)?;
        update_cache(list);
    }
    Ok(())
}

/// Replace the tags of the todo at `index` in `path`.
pub fn set_tags(path: &str, index: usize, tags: &[String]) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    if let Some(entry) = list.get_mut(index) {
        entry.tags = tags.to_vec();
        save_todos(path, &list)?;
        update_cache(list);
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
        update_cache(list);
    }
    Ok(())
}

fn parse_priority(part: &str) -> Option<u8> {
    part.strip_prefix("p=").and_then(|p| p.parse::<u8>().ok())
}

fn parse_tag(part: &str) -> Option<String> {
    part.strip_prefix('#').and_then(|t| {
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

fn parse_index(part: &str) -> Option<usize> {
    part.parse::<usize>().ok()
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
        let data = TODO_DATA.clone();
        let watch_path = TODO_FILE.to_string();
        let watcher = watch_json(&watch_path, {
            let watch_path = watch_path.clone();
            let data_clone = data.clone();
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

    fn filter_entries<'a>(
        &self,
        entries: &mut Vec<(usize, &'a TodoEntry)>,
        filter: &str,
        allow_negative: bool,
    ) {
        let mut filter = filter.trim();
        let mut negative = false;
        if allow_negative {
            if let Some(stripped) = filter.strip_prefix('!') {
                negative = true;
                filter = stripped.trim();
            }
        }

        let tag_filter = filter.starts_with('#');
        if tag_filter {
            let tag = filter.trim_start_matches('#');
            entries.retain(|(_, t)| {
                let has_tag = t.tags.iter().any(|tg| tg.eq_ignore_ascii_case(tag));
                if negative {
                    !has_tag
                } else {
                    has_tag
                }
            });
        } else if !filter.is_empty() {
            entries.retain(|(_, t)| {
                let text_match = self.matcher.fuzzy_match(&t.text, filter).is_some();
                if negative {
                    !text_match
                } else {
                    text_match
                }
            });
        }

        if filter.is_empty() || tag_filter {
            entries.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));
        }
    }

    fn edit_actions(&self, filter: &str) -> Vec<Action> {
        let guard = match self.data.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let mut entries: Vec<(usize, &TodoEntry)> = guard.iter().enumerate().collect();
        self.filter_entries(&mut entries, filter, false);
        entries
            .into_iter()
            .map(|(idx, t)| Action {
                label: format!("{} {}", if t.done { "[x]" } else { "[ ]" }, t.text.clone()),
                desc: "Todo".into(),
                action: format!("todo:edit:{idx}"),
                args: None,
            })
            .collect()
    }

    fn list_actions(&self, filter: &str) -> Vec<Action> {
        let guard = match self.data.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let mut entries: Vec<(usize, &TodoEntry)> = guard.iter().enumerate().collect();
        self.filter_entries(&mut entries, filter.trim(), true);
        entries
            .into_iter()
            .map(|(idx, t)| Action {
                label: format!("{} {}", if t.done { "[x]" } else { "[ ]" }, t.text.clone()),
                desc: "Todo".into(),
                action: format!("todo:done:{idx}"),
                args: None,
            })
            .collect()
    }

    fn tag_actions(&self, rest: &str) -> Vec<Action> {
        let rest = rest.trim();
        let mut parts = rest.split_whitespace();
        if let Some(first) = parts.next() {
            if let Some(idx) = parse_index(first) {
                let tags: Vec<String> = parts.filter_map(parse_tag).collect();
                let tag_str = tags.join(",");
                let guard = match self.data.lock() {
                    Ok(g) => g,
                    Err(_) => return Vec::new(),
                };
                if idx < guard.len() {
                    return vec![Action {
                        label: format!("Set tags for todo {idx}"),
                        desc: "Todo".into(),
                        action: format!("todo:tag:{idx}|{tag_str}"),
                        args: None,
                    }];
                } else {
                    return Vec::new();
                }
            } else {
                let filter = rest;
                let guard = match self.data.lock() {
                    Ok(g) => g,
                    Err(_) => return Vec::new(),
                };
                let mut entries: Vec<(usize, &TodoEntry)> = guard.iter().enumerate().collect();
                let tag_filter = format!("#{filter}");
                self.filter_entries(&mut entries, &tag_filter, false);
                return entries
                    .into_iter()
                    .map(|(idx, t)| Action {
                        label: format!("{} {}", if t.done { "[x]" } else { "[ ]" }, t.text.clone()),
                        desc: "Todo".into(),
                        action: format!("query:todo tag {idx} "),
                        args: None,
                    })
                    .collect();
            }
        }
        Vec::new()
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
            return self.edit_actions(rest.trim());
        }

        if trimmed.eq_ignore_ascii_case("todo view") {
            return vec![Action {
                label: "todo: view list".into(),
                desc: "Todo".into(),
                action: "todo:view".into(),
                args: None,
            }];
        }

        if trimmed.eq_ignore_ascii_case("todo export") {
            return vec![Action {
                label: "Export todo list".into(),
                desc: "Todo".into(),
                action: "todo:export".into(),
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
                    if let Some(p) = parse_priority(part) {
                        priority = p;
                    } else if let Some(tag) = parse_tag(part) {
                        tags.push(tag);
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
                if let (Some(idx), Some(priority)) =
                    (parse_index(idx_str), parse_priority(priority_str))
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
            return self.tag_actions(rest);
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
            return self.list_actions(rest);
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
            Action {
                label: "todo export".into(),
                desc: "Todo".into(),
                action: "query:todo export".into(),
                args: None,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_with(entries: Vec<TodoEntry>) -> TodoPlugin {
        TodoPlugin {
            matcher: SkimMatcherV2::default(),
            data: Arc::new(Mutex::new(entries)),
            watcher: None,
        }
    }

    #[test]
    fn parse_helpers() {
        assert_eq!(parse_priority("p=3"), Some(3));
        assert_eq!(parse_priority("p=x"), None);
        assert_eq!(parse_tag("#a"), Some("a".to_string()));
        assert_eq!(parse_tag("a"), None);
        assert_eq!(parse_index("2"), Some(2));
        assert_eq!(parse_index("a"), None);
    }

    #[test]
    fn filter_entries_tag_and_negative() {
        let plugin = plugin_with(Vec::new());
        let items = vec![
            TodoEntry {
                text: "alpha".into(),
                done: false,
                priority: 1,
                tags: vec!["work".into()],
            },
            TodoEntry {
                text: "beta".into(),
                done: false,
                priority: 2,
                tags: vec![],
            },
        ];
        let mut entries: Vec<(usize, &TodoEntry)> = items.iter().enumerate().collect();
        plugin.filter_entries(&mut entries, "#work", true);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, 0);
        let mut entries: Vec<(usize, &TodoEntry)> = items.iter().enumerate().collect();
        plugin.filter_entries(&mut entries, "!beta", true);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, 0);
    }

    #[test]
    fn edit_actions_filter() {
        let plugin = plugin_with(vec![
            TodoEntry {
                text: "alpha".into(),
                done: false,
                priority: 1,
                tags: vec![],
            },
            TodoEntry {
                text: "beta".into(),
                done: false,
                priority: 2,
                tags: vec![],
            },
        ]);
        let actions = plugin.edit_actions("beta");
        assert_eq!(actions.len(), 1);
        assert!(actions[0].label.contains("beta"));
    }

    #[test]
    fn list_actions_negative_filter() {
        let plugin = plugin_with(vec![
            TodoEntry {
                text: "alpha".into(),
                done: false,
                priority: 1,
                tags: vec![],
            },
            TodoEntry {
                text: "beta".into(),
                done: false,
                priority: 2,
                tags: vec![],
            },
        ]);
        let actions = plugin.list_actions("!beta");
        assert_eq!(actions.len(), 1);
        assert!(actions[0].label.contains("alpha"));
    }

    #[test]
    fn tag_actions_index_and_filter() {
        let plugin = plugin_with(vec![
            TodoEntry {
                text: "alpha".into(),
                done: false,
                priority: 1,
                tags: vec!["x".into()],
            },
            TodoEntry {
                text: "beta".into(),
                done: false,
                priority: 1,
                tags: vec![],
            },
        ]);
        let res = plugin.tag_actions("1 #a #b");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].action, "todo:tag:1|a,b");
        let res = plugin.tag_actions("3 #a");
        assert!(res.is_empty());
        let res = plugin.tag_actions("x");
        assert_eq!(res.len(), 1);
        assert!(res[0].action.starts_with("query:todo tag 0"));
    }

    #[test]
    fn tag_actions_edge_cases() {
        let plugin = plugin_with(vec![
            TodoEntry {
                text: "alpha".into(),
                done: false,
                priority: 1,
                tags: vec!["x".into()],
            },
            TodoEntry {
                text: "beta".into(),
                done: false,
                priority: 1,
                tags: vec![],
            },
        ]);

        // Empty input or whitespace should yield no actions
        assert!(plugin.tag_actions("").is_empty());
        assert!(plugin.tag_actions("   ").is_empty());

        // Index provided without tags should still be handled
        let res = plugin.tag_actions("1");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].action, "todo:tag:1|");

        // Invalid index should behave like a tag filter
        let res = plugin.tag_actions("x");
        assert_eq!(res.len(), 1);
        assert!(res[0].action.starts_with("query:todo tag 0"));
    }
}
