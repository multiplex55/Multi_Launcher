//! Todo plugin and helpers.
//!
//! `TODO_DATA` is a process-wide cache of todos loaded from `todo.json`.
//! Any operation that writes to disk updates this cache, and a `JsonWatcher`
//! refreshes it when the file changes externally. This keeps plugin state and
//! tests synchronized with the latest on-disk data.

use crate::actions::Action;
use crate::common::command::{parse_args, ParseArgsResult};
use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::common::lru::LruCache;
use crate::common::query::parse_query_filters;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, RwLock,
};

pub const TODO_FILE: &str = "todo.json";

static TODO_VERSION: AtomicU64 = AtomicU64::new(0);

const TODO_USAGE: &str = "Usage: todo <add|list|rm|clear|pset|tag|edit|view|export> ...";
const TODO_ADD_USAGE: &str = "Usage: todo add <text> [p=<priority>] [#tag ...]";
const TODO_RM_USAGE: &str = "Usage: todo rm <text>";
const TODO_PSET_USAGE: &str = "Usage: todo pset <index> <priority>";
const TODO_TAG_USAGE: &str = "Usage: todo tag [<filter>] | todo tag <index> [#tag|@tag ...]";
const TODO_CLEAR_USAGE: &str = "Usage: todo clear";
const TODO_VIEW_USAGE: &str = "Usage: todo view";
const TODO_EXPORT_USAGE: &str = "Usage: todo export";

fn usage_action(usage: &str, query: &str) -> Action {
    Action {
        label: usage.into(),
        desc: "Todo".into(),
        action: format!("query:{query}"),
        args: None,
    }
}

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
            if rest.trim().is_empty() {
                let mut actions = vec![Action {
                    label: "todo: edit todos".into(),
                    desc: "Todo".into(),
                    action: "todo:dialog".into(),
                    args: None,
                }];
                actions.extend([
                    Action {
                        label: "todo edit".into(),
                        desc: "Todo".into(),
                        action: "query:todo edit".into(),
                        args: None,
                    },
                    Action {
                        label: "todo list".into(),
                        desc: "Todo".into(),
                        action: "query:todo list".into(),
                        args: None,
                    },
                    Action {
                        label: "todo tag".into(),
                        desc: "Todo".into(),
                        action: "query:todo tag ".into(),
                        args: None,
                    },
                    Action {
                        label: "todo view".into(),
                        desc: "Todo".into(),
                        action: "query:todo view ".into(),
                        args: None,
                    },
                    Action {
                        label: "todo add".into(),
                        desc: "Todo".into(),
                        action: "query:todo add ".into(),
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
                        label: "todo export".into(),
                        desc: "Todo".into(),
                        action: "query:todo export".into(),
                        args: None,
                    },
                ]);
                return actions;
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

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "todo view") {
            if rest.trim().is_empty() {
                return vec![Action {
                    label: "todo: view list".into(),
                    desc: "Todo".into(),
                    action: "todo:view".into(),
                    args: None,
                }];
            }
            return vec![usage_action(TODO_VIEW_USAGE, "todo view")];
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "todo export") {
            if rest.trim().is_empty() {
                return vec![Action {
                    label: "Export todo list".into(),
                    desc: "Todo".into(),
                    action: "todo:export".into(),
                    args: None,
                }];
            }
            return vec![usage_action(TODO_EXPORT_USAGE, "todo export")];
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "todo clear") {
            if rest.trim().is_empty() {
                return vec![Action {
                    label: "Clear completed todos".into(),
                    desc: "Todo".into(),
                    action: "todo:clear".into(),
                    args: None,
                }];
            }
            return vec![usage_action(TODO_CLEAR_USAGE, "todo clear")];
        }

        if trimmed.eq_ignore_ascii_case("todo add") {
            return vec![
                Action {
                    label: "todo: edit todos".into(),
                    desc: "Todo".into(),
                    action: "todo:dialog".into(),
                    args: None,
                },
                usage_action(TODO_ADD_USAGE, "todo add "),
            ];
        }

        const ADD_PREFIX: &str = "todo add ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, ADD_PREFIX) {
            let rest = rest.trim();
            let args: Vec<&str> = rest.split_whitespace().collect();
            match parse_args(&args, TODO_ADD_USAGE, |args| {
                let mut priority: u8 = 0;
                let mut tags: Vec<String> = Vec::new();
                let mut words: Vec<String> = Vec::new();
                for part in args {
                    if let Some(p) = part.strip_prefix("p=") {
                        if let Ok(n) = p.parse::<u8>() {
                            priority = n;
                        }
                    } else if let Some(tag) =
                        part.strip_prefix('#').or_else(|| part.strip_prefix('@'))
                    {
                        if !tag.is_empty() {
                            tags.push(tag.to_string());
                        }
                    } else {
                        words.push((*part).to_string());
                    }
                }
                let text = words.join(" ");
                if text.is_empty() {
                    return None;
                }
                Some((text, priority, tags))
            }) {
                ParseArgsResult::Parsed((text, priority, tags)) => {
                    let tag_str = tags.join(",");
                    return vec![Action {
                        label: format!("Add todo {text}"),
                        desc: "Todo".into(),
                        action: format!("todo:add:{text}|{priority}|{tag_str}"),
                        args: None,
                    }];
                }
                ParseArgsResult::Usage(usage) => {
                    return vec![usage_action(&usage, "todo add ")];
                }
            }
        }

        const PSET_PREFIX: &str = "todo pset ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, PSET_PREFIX) {
            let rest = rest.trim();
            let args: Vec<&str> = rest.split_whitespace().collect();
            match parse_args(&args, TODO_PSET_USAGE, |args| {
                let (idx_str, priority_str) = (args.get(0)?, args.get(1)?);
                let idx = idx_str.parse::<usize>().ok()?;
                let priority = priority_str.parse::<u8>().ok()?;
                Some((idx, priority))
            }) {
                ParseArgsResult::Parsed((idx, priority)) => {
                    return vec![Action {
                        label: format!("Set priority {priority} for todo {idx}"),
                        desc: "Todo".into(),
                        action: format!("todo:pset:{idx}|{priority}"),
                        args: None,
                    }];
                }
                ParseArgsResult::Usage(usage) => {
                    return vec![usage_action(&usage, "todo pset ")];
                }
            }
        }

        const TAG_PREFIX: &str = "todo tag";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, TAG_PREFIX) {
            let rest = rest.trim();
            let args: Vec<&str> = rest.split_whitespace().collect();

            // `todo tag <index> [#tag|@tag ...]` updates tags for a specific todo.
            if let Some(idx) = args.first().and_then(|s| s.parse::<usize>().ok()) {
                let mut tags: Vec<String> = Vec::new();
                for t in args.iter().skip(1) {
                    if let Some(tag) = t.strip_prefix('#').or_else(|| t.strip_prefix('@')) {
                        let tag = tag.trim();
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

            // Otherwise, `todo tag [<filter>]` lists all known tags and lets you drill into `todo list`.
            let filter = if rest.is_empty() {
                None
            } else {
                let mut filter = rest;
                if let Some(stripped) = filter.strip_prefix("tag:") {
                    filter = stripped.trim();
                }
                if let Some(stripped) =
                    filter.strip_prefix('#').or_else(|| filter.strip_prefix('@'))
                {
                    filter = stripped.trim();
                }
                if filter.is_empty() {
                    None
                } else {
                    Some(filter)
                }
            };

            let guard = match self.data.read() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };

            let mut counts: HashMap<String, (String, usize)> = HashMap::new();
            for entry in guard.iter() {
                let mut seen: HashSet<String> = HashSet::new();
                for tag in &entry.tags {
                    let key = tag.to_lowercase();
                    if !seen.insert(key.clone()) {
                        continue;
                    }
                    let e = counts.entry(key).or_insert((tag.clone(), 0));
                    e.1 += 1;
                }
            }

            let mut tags: Vec<(String, usize)> = counts
                .into_values()
                .map(|(display, count)| (display, count))
                .collect();

            if let Some(filter) = filter {
                tags.retain(|(tag, _)| self.matcher.fuzzy_match(tag, filter).is_some());
            }

            tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

            return tags
                .into_iter()
                .map(|(tag, count)| Action {
                    label: format!("#{tag} ({count})"),
                    desc: "Todo".into(),
                    action: format!("query:todo list #{tag}"),
                    args: None,
                })
                .collect();
        }


        const RM_PREFIX: &str = "todo rm ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, RM_PREFIX) {
            let filter = rest.trim();
            if filter.is_empty() {
                return vec![usage_action(TODO_RM_USAGE, "todo rm ")];
            }
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
            let filter = rest.trim();
            let guard = match self.data.read() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            let mut entries: Vec<(usize, &TodoEntry)> = guard.iter().enumerate().collect();

            let filters = parse_query_filters(filter, &["@", "#", "tag:"]);
            let text_filter = filters.remaining_tokens.join(" ");
            let has_tag_filter =
                !filters.include_tags.is_empty() || !filters.exclude_tags.is_empty();

            // Tag filters run first, then text filters apply fuzzy matching against remaining text.
            if !filters.include_tags.is_empty() {
                entries.retain(|(_, t)| {
                    filters.include_tags.iter().all(|requested| {
                        t.tags.iter().any(|tag| tag.eq_ignore_ascii_case(requested))
                    })
                });
            }

            if !filters.exclude_tags.is_empty() {
                entries.retain(|(_, t)| {
                    !filters
                        .exclude_tags
                        .iter()
                        .any(|excluded| t.tags.iter().any(|tag| tag.eq_ignore_ascii_case(excluded)))
                });
            }

            if !text_filter.is_empty() {
                entries.retain(|(_, t)| {
                    let text_match = self.matcher.fuzzy_match(&t.text, &text_filter).is_some();
                    if filters.negate_text {
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

        if crate::common::strip_prefix_ci(trimmed, "todo").is_some() {
            return vec![usage_action(TODO_USAGE, "todo ")];
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
        let trimmed = query.trim_start();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn set_todos(entries: Vec<TodoEntry>) -> Vec<TodoEntry> {
        let original = TODO_DATA.read().unwrap().clone();
        let mut guard = TODO_DATA.write().unwrap();
        *guard = entries;
        original
    }

    #[test]
    fn list_filters_by_tags_and_text() {
        let original = set_todos(vec![
            TodoEntry {
                text: "foo alpha".into(),
                done: false,
                priority: 3,
                tags: vec!["testing".into(), "ui".into()],
            },
            TodoEntry {
                text: "bar beta".into(),
                done: false,
                priority: 1,
                tags: vec!["testing".into()],
            },
            TodoEntry {
                text: "foo gamma".into(),
                done: false,
                priority: 2,
                tags: vec!["ui".into()],
            },
            TodoEntry {
                text: "urgent delta".into(),
                done: false,
                priority: 4,
                tags: vec!["high priority".into(), "chore".into()],
            },
        ]);

        let plugin = TodoPlugin {
            matcher: SkimMatcherV2::default(),
            data: TODO_DATA.clone(),
            cache: TODO_CACHE.clone(),
            watcher: None,
        };

        let list_testing = plugin.search_internal("todo list @testing");
        let labels_testing: Vec<&str> = list_testing.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_testing, vec!["[ ] foo alpha", "[ ] bar beta"]);

        let list_testing_hash = plugin.search_internal("todo list #testing");
        let labels_testing_hash: Vec<&str> =
            list_testing_hash.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_testing_hash, vec!["[ ] foo alpha", "[ ] bar beta"]);

        let list_testing_ui = plugin.search_internal("todo list @testing @ui");
        let labels_testing_ui: Vec<&str> =
            list_testing_ui.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_testing_ui, vec!["[ ] foo alpha"]);

        let list_testing_ui_hash = plugin.search_internal("todo list #testing #ui");
        let labels_testing_ui_hash: Vec<&str> =
            list_testing_ui_hash.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_testing_ui_hash, vec!["[ ] foo alpha"]);

        let list_negated = plugin.search_internal("todo list !foo @testing");
        let labels_negated: Vec<&str> = list_negated.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_negated, vec!["[ ] bar beta"]);

        let list_quoted_tag = plugin.search_internal("todo list tag:\"high priority\"");
        let labels_quoted: Vec<&str> = list_quoted_tag.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_quoted, vec!["[ ] urgent delta"]);

        let list_exclude_tag = plugin.search_internal("todo list !tag:ui");
        let labels_exclude: Vec<&str> = list_exclude_tag.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_exclude, vec!["[ ] urgent delta", "[ ] bar beta"]);

        if let Ok(mut guard) = TODO_DATA.write() {
            *guard = original;
        }
    }

    #[test]
    fn tag_command_lists_tags_and_filters() {
        let original = set_todos(vec![
            TodoEntry {
                text: "foo alpha".into(),
                done: false,
                priority: 3,
                tags: vec!["testing".into(), "ui".into()],
            },
            TodoEntry {
                text: "bar beta".into(),
                done: false,
                priority: 1,
                tags: vec!["testing".into()],
            },
            TodoEntry {
                text: "foo gamma".into(),
                done: false,
                priority: 2,
                tags: vec!["ui".into()],
            },
            TodoEntry {
                text: "urgent delta".into(),
                done: false,
                priority: 4,
                tags: vec!["high priority".into(), "chore".into()],
            },
        ]);

        let plugin = TodoPlugin {
            matcher: SkimMatcherV2::default(),
            data: TODO_DATA.clone(),
            cache: TODO_CACHE.clone(),
            watcher: None,
        };

        let tags = plugin.search_internal("todo tag");
        let labels: Vec<&str> = tags.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["#testing (2)", "#ui (2)", "#chore (1)", "#high priority (1)"]
        );
        let actions: Vec<&str> = tags.iter().map(|a| a.action.as_str()).collect();
        assert_eq!(
            actions,
            vec![
                "query:todo list #testing",
                "query:todo list #ui",
                "query:todo list #chore",
                "query:todo list #high priority"
            ]
        );

        let tags_ui = plugin.search_internal("todo tag @ui");
        let labels_ui: Vec<&str> = tags_ui.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_ui, vec!["#ui (2)"]);

        let tags_ui_hash = plugin.search_internal("todo tag #ui");
        let labels_ui_hash: Vec<&str> = tags_ui_hash.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_ui_hash, vec!["#ui (2)"]);

        let tags_ui_tag = plugin.search_internal("todo tag tag:ui");
        let labels_ui_tag: Vec<&str> = tags_ui_tag.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels_ui_tag, vec!["#ui (2)"]);

        if let Ok(mut guard) = TODO_DATA.write() {
            *guard = original;
        }
    }

    #[test]
    fn todo_root_query_with_space_lists_subcommands_in_order() {
        let plugin = TodoPlugin {
            matcher: SkimMatcherV2::default(),
            data: TODO_DATA.clone(),
            cache: TODO_CACHE.clone(),
            watcher: None,
        };

        let actions = plugin.search_internal("todo ");
        let labels: Vec<&str> = actions.iter().map(|a| a.label.as_str()).collect();
        let actions_list: Vec<&str> = actions.iter().map(|a| a.action.as_str()).collect();

        assert_eq!(
            labels,
            vec![
                "todo: edit todos",
                "todo edit",
                "todo list",
                "todo tag",
                "todo view",
                "todo add",
                "todo rm",
                "todo clear",
                "todo pset",
                "todo export",
            ]
        );
        assert_eq!(actions_list[0], "todo:dialog");
    }

}
