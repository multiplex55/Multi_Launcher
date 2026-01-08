//! Todo plugin and helpers.
//!
//! `TODO_DATA` is a process-wide cache of todos loaded from `todo.json`.
//! Any operation that writes to disk updates this cache, and a `JsonWatcher`
//! refreshes it when the file changes externally. This keeps plugin state and
//! tests synchronized with the latest on-disk data.

use crate::actions::Action;
use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::common::lru::LruCache;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, RwLock,
};

pub const TODO_FILE: &str = "todo.json";

static TODO_VERSION: AtomicU64 = AtomicU64::new(0);

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
pub static TODO_DATA: Lazy<Arc<RwLock<Vec<TodoEntry>>>> =
    Lazy::new(|| Arc::new(RwLock::new(load_todos(TODO_FILE).unwrap_or_default())));

static TODO_CACHE: Lazy<Arc<RwLock<LruCache<String, Vec<Action>>>>> =
    Lazy::new(|| Arc::new(RwLock::new(LruCache::new(64))));

fn invalidate_todo_cache() {
    if let Ok(mut cache) = TODO_CACHE.write() {
        cache.clear();
    }
}

fn bump_todo_version() {
    TODO_VERSION.fetch_add(1, Ordering::SeqCst);
}

pub fn todo_version() -> u64 {
    TODO_VERSION.load(Ordering::SeqCst)
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

fn update_cache(list: Vec<TodoEntry>) {
    if let Ok(mut lock) = TODO_DATA.write() {
        *lock = list;
    }
    invalidate_todo_cache();
    bump_todo_version();
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

pub struct TodoPlugin {
    matcher: SkimMatcherV2,
    data: Arc<RwLock<Vec<TodoEntry>>>,
    cache: Arc<RwLock<LruCache<String, Vec<Action>>>>,
    #[allow(dead_code)]
    watcher: Option<JsonWatcher>,
}

impl TodoPlugin {
    /// Create a new todo plugin with a fuzzy matcher.
    pub fn new() -> Self {
        let data = TODO_DATA.clone();
        let cache = TODO_CACHE.clone();
        let watch_path = TODO_FILE.to_string();
        let watcher = watch_json(&watch_path, {
            let watch_path = watch_path.clone();
            let data_clone = data.clone();
            let cache_clone = cache.clone();
            move || {
                if let Ok(list) = load_todos(&watch_path) {
                    if let Ok(mut lock) = data_clone.write() {
                        *lock = list;
                    }
                    if let Ok(mut c) = cache_clone.write() {
                        c.clear();
                    }
                    bump_todo_version();
                }
            }
        })
        .ok();
        Self {
            matcher: SkimMatcherV2::default(),
            data,
            cache,
            watcher,
        }
    }

    fn search_internal(&self, trimmed: &str) -> Vec<Action> {
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
            let guard = match self.data.read() {
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
                    if let Some(p) = part.strip_prefix("p=") {
                        if let Ok(n) = p.parse::<u8>() {
                            priority = n;
                        }
                    } else if let Some(tag) = part.strip_prefix('#').or_else(|| part.strip_prefix('@'))
                    {
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
            if let Some(first) = parts.next() {
                if let Ok(idx) = first.parse::<usize>() {
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
                } else {
                    let filter = rest;
                    let guard = match self.data.read() {
                        Ok(g) => g,
                        Err(_) => return Vec::new(),
                    };
                    let mut entries: Vec<(usize, &TodoEntry)> = guard.iter().enumerate().collect();
                    entries
                        .retain(|(_, t)| t.tags.iter().any(|tg| tg.eq_ignore_ascii_case(filter)));
                    entries.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));
                    return entries
                        .into_iter()
                        .map(|(idx, t)| Action {
                            label: format!(
                                "{} {}",
                                if t.done { "[x]" } else { "[ ]" },
                                t.text.clone()
                            ),
                            desc: "Todo".into(),
                            action: format!("query:todo tag {idx} "),
                            args: None,
                        })
                        .collect();
                }
            }
        }

        const RM_PREFIX: &str = "todo rm ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, RM_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.read() {
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
            let mut filter = rest.trim();
            let guard = match self.data.read() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            let mut entries: Vec<(usize, &TodoEntry)> = guard.iter().enumerate().collect();

            let mut requested_tags: Vec<&str> = Vec::new();
            let mut text_tokens: Vec<&str> = Vec::new();
            let mut negative = false;
            for token in filter.split_whitespace() {
                if let Some(stripped) = token.strip_prefix('!') {
                    if !negative
                        && !stripped.starts_with('@')
                        && !stripped.starts_with('#')
                        && text_tokens.is_empty()
                    {
                        negative = true;
                        if !stripped.is_empty() {
                            text_tokens.push(stripped);
                        }
                        continue;
                    }
                }

                if let Some(tag) = token.strip_prefix('@').or_else(|| token.strip_prefix('#')) {
                    if !tag.is_empty() {
                        requested_tags.push(tag);
                    }
                } else {
                    text_tokens.push(token);
                }
            }

            let text_filter = text_tokens.join(" ");
            let has_tag_filter = !requested_tags.is_empty();

            // Tag filters run first, then text filters apply fuzzy matching against remaining text.
            if has_tag_filter {
                entries.retain(|(_, t)| {
                    requested_tags.iter().all(|requested| {
                        t.tags
                            .iter()
                            .any(|tag| tag.eq_ignore_ascii_case(requested))
                    })
                });
            }

            if !text_filter.is_empty() {
                entries.retain(|(_, t)| {
                    let text_match = self.matcher.fuzzy_match(&t.text, &text_filter).is_some();
                    if negative {
                        !text_match
                    } else {
                        text_match
                    }
                });
            }

            if text_filter.is_empty() || has_tag_filter {
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
}

impl Default for TodoPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for TodoPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        let key = trimmed.to_string();
        if let Ok(mut cache) = self.cache.write() {
            if let Some(res) = cache.get(&key).cloned() {
                return res;
            }
        }

        let result = self.search_internal(trimmed);

        if let Ok(mut cache) = self.cache.write() {
            cache.insert(key, result.clone());
        }

        result
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
