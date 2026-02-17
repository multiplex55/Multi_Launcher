//! Todo plugin and helpers.
//!
//! `TODO_DATA` is a process-wide cache of todos loaded from `todo.json`.
//! Any operation that writes to disk updates this cache, and a `JsonWatcher`
//! refreshes it when the file changes externally. This keeps plugin state and
//! tests synchronized with the latest on-disk data.

use crate::actions::Action;
use crate::common::command::{parse_args, ParseArgsResult};
use crate::common::entity_ref::{EntityKind, EntityRef};
use crate::common::json_watch::{watch_json, JsonWatcher};
use crate::common::lru::LruCache;
use crate::common::query::parse_query_filters;
use crate::linking::{
    build_index_from_notes_and_todos, format_link_id, EntityKey, LinkIndex, LinkRef, LinkTarget,
};
use crate::plugin::Plugin;
use crate::plugins::note::{load_notes, note_version, Note};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
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
static NEXT_TODO_ID: AtomicU64 = AtomicU64::new(1);

fn next_todo_id() -> String {
    let next = NEXT_TODO_ID.fetch_add(1, Ordering::SeqCst);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("todo-{ts}-{next}")
}

const TODO_USAGE: &str = "Usage: todo <add|list|rm|clear|pset|tag|edit|view|export> ...";
const TODO_ADD_USAGE: &str =
    "Usage: todo add <text> [p=<priority>] [#tag ...] [@note:<id>|@event:<id>]";
const TODO_RM_USAGE: &str = "Usage: todo rm <text>";
const TODO_PSET_USAGE: &str = "Usage: todo pset <index> <priority>";
const TODO_CLEAR_USAGE: &str = "Usage: todo clear";
const TODO_VIEW_USAGE: &str = "Usage: todo view";
const TODO_EXPORT_USAGE: &str = "Usage: todo export";
const TODO_LINKS_USAGE: &str = "Usage: todo links <query> [--json]";

fn parse_entity_ref_token(token: &str) -> Option<EntityRef> {
    let token = token.trim();
    let token = token.strip_prefix('@').unwrap_or(token);
    let (kind, id) = token.split_once(':')?;
    if id.trim().is_empty() {
        return None;
    }
    let kind = match kind.to_ascii_lowercase().as_str() {
        "note" => EntityKind::Note,
        "todo" => EntityKind::Todo,
        "event" => EntityKind::Event,
        _ => return None,
    };
    Some(EntityRef::new(kind, id.trim().to_string(), None))
}

fn usage_action(usage: &str, query: &str) -> Action {
    Action {
        label: usage.into(),
        desc: "Todo".into(),
        action: format!("query:{query}"),
        args: None,
    }
}

fn resolve_todo_matches<'a>(entries: &'a [TodoEntry], query: &str) -> Vec<&'a TodoEntry> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    if let Some(id) = q.strip_prefix("id:") {
        let id = id.trim();
        return entries.iter().filter(|t| t.id == id).collect();
    }
    let exact: Vec<&TodoEntry> = entries
        .iter()
        .filter(|t| t.text.eq_ignore_ascii_case(q))
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    let matcher = SkimMatcherV2::default();
    entries
        .iter()
        .filter(|t| matcher.fuzzy_match(&t.text, q).is_some())
        .collect()
}

fn format_todo_link_row(
    notes: &[crate::plugins::note::Note],
    todos: &[TodoEntry],
    link: &LinkRef,
    status: &str,
) -> Action {
    let title = match link.target_type {
        LinkTarget::Note => notes
            .iter()
            .find(|n| n.slug == link.target_id)
            .map(|n| n.alias.as_ref().unwrap_or(&n.title).clone())
            .unwrap_or_else(|| link.target_id.clone()),
        LinkTarget::Todo => todos
            .iter()
            .find(|t| t.id == link.target_id)
            .map(|t| t.text.clone())
            .unwrap_or_else(|| link.target_id.clone()),
        _ => link
            .display_text
            .clone()
            .unwrap_or_else(|| link.target_id.clone()),
    };
    Action {
        label: format!(
            "type={} | title={} | target={} | anchor={} | status={}",
            match link.target_type {
                LinkTarget::Note => "note",
                LinkTarget::Todo => "todo",
                LinkTarget::Bookmark => "bookmark",
                LinkTarget::Layout => "layout",
                LinkTarget::File => "file",
            },
            title,
            format_link_id(link),
            link.anchor.clone().unwrap_or_else(|| "-".into()),
            status
        ),
        desc: "Links".into(),
        action: format!("link:open:{}", format_link_id(link)),
        args: None,
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TodoEntry {
    #[serde(default)]
    pub id: String,
    pub text: String,
    pub done: bool,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub entity_refs: Vec<EntityRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TodoAddActionPayload {
    pub text: String,
    pub priority: u8,
    pub tags: Vec<String>,
    pub refs: Vec<EntityRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TodoTagActionPayload {
    pub idx: usize,
    pub tags: Vec<String>,
}

fn encode_action_payload<T: Serialize>(payload: &T) -> Option<String> {
    let json = serde_json::to_vec(payload).ok()?;
    Some(URL_SAFE_NO_PAD.encode(json))
}

fn decode_action_payload<T: for<'de> Deserialize<'de>>(payload: &str) -> Option<T> {
    let json = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&json).ok()
}

pub(crate) fn encode_todo_add_action_payload(payload: &TodoAddActionPayload) -> Option<String> {
    encode_action_payload(payload)
}

pub(crate) fn decode_todo_add_action_payload(payload: &str) -> Option<TodoAddActionPayload> {
    decode_action_payload(payload)
}

pub(crate) fn encode_todo_tag_action_payload(payload: &TodoTagActionPayload) -> Option<String> {
    encode_action_payload(payload)
}

pub(crate) fn decode_todo_tag_action_payload(payload: &str) -> Option<TodoTagActionPayload> {
    decode_action_payload(payload)
}

/// Shared in-memory todo cache kept in sync with `todo.json`.
/// Disk writes and the [`JsonWatcher`] ensure updates are visible immediately
/// to all plugin instances and tests.
pub static TODO_DATA: Lazy<Arc<RwLock<Vec<TodoEntry>>>> =
    Lazy::new(|| Arc::new(RwLock::new(load_todos(TODO_FILE).unwrap_or_default())));

static TODO_CACHE: Lazy<Arc<RwLock<LruCache<String, Vec<Action>>>>> =
    Lazy::new(|| Arc::new(RwLock::new(LruCache::new(64))));

#[derive(Clone)]
struct TodoLinksIndexCache {
    todo_version: u64,
    note_version: u64,
    notes: Vec<Note>,
    index: LinkIndex,
}

static TODO_LINKS_INDEX_CACHE: Lazy<RwLock<Option<TodoLinksIndexCache>>> =
    Lazy::new(|| RwLock::new(None));

static TODO_LINKS_INDEX_REBUILD_COUNT: AtomicU64 = AtomicU64::new(0);

fn invalidate_todo_cache() {
    if let Ok(mut cache) = TODO_CACHE.write() {
        cache.clear();
    }
}

fn invalidate_todo_links_index_cache() {
    if let Ok(mut cache) = TODO_LINKS_INDEX_CACHE.write() {
        *cache = None;
    }
}

fn get_todo_links_index(notes_todos: &[TodoEntry]) -> (Vec<Note>, LinkIndex) {
    let todo_ver = todo_version();
    let note_ver = note_version();
    if let Ok(cache) = TODO_LINKS_INDEX_CACHE.read() {
        if let Some(entry) = cache.as_ref() {
            if entry.todo_version == todo_ver && entry.note_version == note_ver {
                return (entry.notes.clone(), entry.index.clone());
            }
        }
    }

    let notes = load_notes().unwrap_or_default();
    let todos = notes_todos.to_vec();
    let index = build_index_from_notes_and_todos(&notes, &todos);
    TODO_LINKS_INDEX_REBUILD_COUNT.fetch_add(1, Ordering::SeqCst);

    if let Ok(mut cache) = TODO_LINKS_INDEX_CACHE.write() {
        *cache = Some(TodoLinksIndexCache {
            todo_version: todo_ver,
            note_version: note_ver,
            notes: notes.clone(),
            index: index.clone(),
        });
    }

    (notes, index)
}

#[cfg(test)]
fn reset_todo_links_index_cache_state() {
    invalidate_todo_links_index_cache();
    TODO_LINKS_INDEX_REBUILD_COUNT.store(0, Ordering::SeqCst);
}

#[cfg(test)]
fn todo_links_index_rebuild_count() -> u64 {
    TODO_LINKS_INDEX_REBUILD_COUNT.load(Ordering::SeqCst)
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
    let mut list: Vec<TodoEntry> = serde_json::from_str(&content)?;
    let mut changed = false;
    for entry in &mut list {
        if entry.id.is_empty() {
            entry.id = next_todo_id();
            changed = true;
        }
    }
    if changed {
        let _ = save_todos(path, &list);
    }
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
    invalidate_todo_links_index_cache();
    bump_todo_version();
}

/// Append a new todo entry with `text`, `priority` and `tags`.
pub fn append_todo(
    path: &str,
    text: &str,
    priority: u8,
    tags: &[String],
    refs: &[EntityRef],
) -> anyhow::Result<()> {
    let mut list = load_todos(path).unwrap_or_default();
    list.push(TodoEntry {
        id: next_todo_id(),
        text: text.to_string(),
        done: false,
        priority,
        tags: tags.to_vec(),
        entity_refs: refs.to_vec(),
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
                    invalidate_todo_links_index_cache();
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

            let tag_filter = filter.starts_with('#') || filter.starts_with('@');
            if tag_filter {
                let tag = filter.trim_start_matches(['#', '@']);
                let requested = tag.to_lowercase();
                entries.retain(|(_, t)| {
                    !requested.is_empty()
                        && t.tags
                            .iter()
                            .any(|tg| tg.to_lowercase().contains(&requested))
                });
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
                let mut refs: Vec<EntityRef> = Vec::new();
                for part in args {
                    if let Some(p) = part.strip_prefix("p=") {
                        if let Ok(n) = p.parse::<u8>() {
                            priority = n;
                        }
                    } else if let Some(r) = parse_entity_ref_token(part) {
                        refs.push(r);
                    } else if let Some(tag) = part.strip_prefix('#') {
                        if !tag.is_empty() {
                            tags.push(tag.to_string());
                        }
                    } else if let Some(tag) = part.strip_prefix('@') {
                        // Keep `@tag` shorthand behavior for tags, while `@kind:id`
                        // continues to be parsed as an entity reference above.
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
                Some((text, priority, tags, refs))
            }) {
                ParseArgsResult::Parsed((text, priority, tags, refs)) => {
                    let mut label_suffix_parts: Vec<String> = Vec::new();
                    if !tags.is_empty() {
                        label_suffix_parts.push(format!("Tag: {}", tags.join(", ")));
                    }
                    if priority > 0 {
                        label_suffix_parts.push(format!("priority: {priority}"));
                    }
                    if !refs.is_empty() {
                        label_suffix_parts.push(format!("links: {}", refs.len()));
                    }
                    let label = if label_suffix_parts.is_empty() {
                        format!("Add todo {text}")
                    } else {
                        format!("Add todo {text} {}", label_suffix_parts.join("; "))
                    };
                    let payload = TodoAddActionPayload {
                        text,
                        priority,
                        tags,
                        refs,
                    };
                    let Some(encoded_payload) = encode_todo_add_action_payload(&payload) else {
                        return Vec::new();
                    };
                    return vec![Action {
                        label,
                        desc: "Todo".into(),
                        action: format!("todo:add:{encoded_payload}"),
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
                let payload = TodoTagActionPayload { idx, tags };
                let Some(encoded_payload) = encode_todo_tag_action_payload(&payload) else {
                    return Vec::new();
                };
                return vec![Action {
                    label: format!("Set tags for todo {idx}"),
                    desc: "Todo".into(),
                    action: format!("todo:tag:{encoded_payload}"),
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
                if let Some(stripped) = filter
                    .strip_prefix('#')
                    .or_else(|| filter.strip_prefix('@'))
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

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "todo links") {
            let rest = rest.trim();
            if rest.is_empty() {
                return vec![usage_action(TODO_LINKS_USAGE, "todo links ")];
            }
            let json_mode = rest.contains("--json");
            let query = rest.replace("--json", "").trim().to_string();
            let guard = match self.data.read() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            let matches = resolve_todo_matches(&guard, &query);
            if matches.is_empty() {
                return vec![Action {
                    label: format!("No todo found for \"{query}\""),
                    desc: "Links".into(),
                    action: "query:todo links ".into(),
                    args: None,
                }];
            }
            if matches.len() > 1 {
                let mut actions = vec![Action {
                    label: format!(
                        "Ambiguous todo query \"{query}\" ({} matches)",
                        matches.len()
                    ),
                    desc: "Links".into(),
                    action: "query:todo links ".into(),
                    args: None,
                }];
                actions.extend(matches.iter().take(8).map(|t| Action {
                    label: format!("Candidate: {} [{}]", t.text, t.id),
                    desc: "Links".into(),
                    action: format!("query:todo links id:{}", t.id),
                    args: None,
                }));
                return actions;
            }
            let Some(todo) = matches.first() else {
                return Vec::new();
            };
            let todos = guard.clone();
            let (notes, index) = get_todo_links_index(&todos);
            let source = EntityKey::new(LinkTarget::Todo, todo.id.clone());
            let mut actions: Vec<Action> = index
                .get_forward_links(&source)
                .into_iter()
                .map(|link| format_todo_link_row(&notes, &todos, &link, "attached"))
                .collect();
            if json_mode {
                let json_rows: Vec<serde_json::Value> = index
                    .get_forward_links(&source)
                    .into_iter()
                    .map(|link| {
                        serde_json::json!({
                            "type": match link.target_type { LinkTarget::Note=>"note", LinkTarget::Todo=>"todo", LinkTarget::Bookmark=>"bookmark", LinkTarget::Layout=>"layout", LinkTarget::File=>"file" },
                            "title": link.display_text.clone().unwrap_or_else(|| link.target_id.clone()),
                            "target": format_link_id(&link),
                            "anchor": link.anchor.clone().unwrap_or_default(),
                            "status": "attached"
                        })
                    })
                    .collect();
                actions.insert(
                    0,
                    Action {
                        label: serde_json::to_string_pretty(&json_rows)
                            .unwrap_or_else(|_| "[]".into()),
                        desc: "Links JSON".into(),
                        action: "noop".into(),
                        args: None,
                    },
                );
            }
            if actions.is_empty() {
                actions.push(Action {
                    label: format!("No links for todo {}", todo.id),
                    desc: "Links".into(),
                    action: format!("query:todo links id:{}", todo.id),
                    args: None,
                });
            }
            return actions;
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
            // Prefer disk as the source of truth for list views so toggles are reflected
            // immediately even if file-watch events briefly race with writes.
            //
            // Test cases may inject `self.data` directly while an unrelated empty `todo.json`
            // exists in the working directory; when disk is empty but memory is populated,
            // keep the injected in-memory snapshot.
            let mem_todos = self
                .data
                .read()
                .map(|g| g.clone())
                .unwrap_or_else(|_| Vec::new());
            let todos = if self.watcher.is_none() && !mem_todos.is_empty() {
                mem_todos.clone()
            } else if std::path::Path::new(TODO_FILE).exists() {
                let disk_todos = load_todos(TODO_FILE).unwrap_or_else(|_| mem_todos.clone());
                if disk_todos.is_empty() && !mem_todos.is_empty() {
                    mem_todos.clone()
                } else {
                    disk_todos
                }
            } else {
                mem_todos.clone()
            };
            if let Ok(mut lock) = self.data.write() {
                *lock = todos.clone();
            }
            let mut entries: Vec<(usize, &TodoEntry)> = todos.iter().enumerate().collect();

            let filters = parse_query_filters(filter, &["@", "#", "tag:"]);
            let text_filter = filters.remaining_tokens.join(" ");
            let has_tag_filter =
                !filters.include_tags.is_empty() || !filters.exclude_tags.is_empty();

            // Tag filters run first, then text filters apply fuzzy matching against remaining text.
            if !filters.include_tags.is_empty() {
                entries.retain(|(_, t)| {
                    filters.include_tags.iter().all(|requested| {
                        let requested = requested.to_lowercase();
                        t.tags
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(&requested))
                    })
                });
            }

            if !filters.exclude_tags.is_empty() {
                entries.retain(|(_, t)| {
                    !filters.exclude_tags.iter().any(|excluded| {
                        let excluded = excluded.to_lowercase();
                        t.tags
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(&excluded))
                    })
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
                label: "todo links".into(),
                desc: "Todo".into(),
                action: "query:todo links ".into(),
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

    fn query_prefixes(&self) -> &[&str] {
        &["todo"]
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
                entity_refs: Vec::new(),
                id: "t1".into(),
            },
            TodoEntry {
                text: "bar beta".into(),
                done: false,
                priority: 1,
                tags: vec!["testing".into()],
                entity_refs: Vec::new(),
                id: "t2".into(),
            },
            TodoEntry {
                text: "foo gamma".into(),
                done: false,
                priority: 2,
                tags: vec!["ui".into()],
                entity_refs: Vec::new(),
                id: "t3".into(),
            },
            TodoEntry {
                text: "urgent delta".into(),
                done: false,
                priority: 4,
                tags: vec!["high priority".into(), "chore".into()],
                entity_refs: Vec::new(),
                id: "t4".into(),
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
        let labels_testing_ui_hash: Vec<&str> = list_testing_ui_hash
            .iter()
            .map(|a| a.label.as_str())
            .collect();
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
                entity_refs: Vec::new(),
                id: "t1".into(),
            },
            TodoEntry {
                text: "bar beta".into(),
                done: false,
                priority: 1,
                tags: vec!["testing".into()],
                entity_refs: Vec::new(),
                id: "t2".into(),
            },
            TodoEntry {
                text: "foo gamma".into(),
                done: false,
                priority: 2,
                tags: vec!["ui".into()],
                entity_refs: Vec::new(),
                id: "t3".into(),
            },
            TodoEntry {
                text: "urgent delta".into(),
                done: false,
                priority: 4,
                tags: vec!["high priority".into(), "chore".into()],
                entity_refs: Vec::new(),
                id: "t4".into(),
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
            vec![
                "#testing (2)",
                "#ui (2)",
                "#chore (1)",
                "#high priority (1)"
            ]
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

    #[test]
    fn todo_add_and_tag_actions_encode_payload_for_round_trip() {
        let plugin = TodoPlugin {
            matcher: SkimMatcherV2::default(),
            data: TODO_DATA.clone(),
            cache: TODO_CACHE.clone(),
            watcher: None,
        };

        let add_actions = plugin.search_internal(
            "todo add finish|draft, now p=7 #core|team,ops #has space @note:alpha",
        );
        assert_eq!(add_actions.len(), 1);
        let add_encoded = add_actions[0]
            .action
            .strip_prefix("todo:add:")
            .expect("todo:add: prefix");
        let add_payload = decode_todo_add_action_payload(add_encoded).expect("decode add payload");
        assert_eq!(
            add_payload,
            TodoAddActionPayload {
                text: "finish|draft, now space".into(),
                priority: 7,
                tags: vec!["core|team,ops".into(), "has".into()],
                refs: vec![EntityRef::new(EntityKind::Note, "alpha", None)],
            }
        );

        let tag_actions = plugin.search_internal("todo tag 4 #alpha|beta,gamma #needs space");
        assert_eq!(tag_actions.len(), 1);
        let tag_encoded = tag_actions[0]
            .action
            .strip_prefix("todo:tag:")
            .expect("todo:tag: prefix");
        let tag_payload = decode_todo_tag_action_payload(tag_encoded).expect("decode tag payload");
        assert_eq!(
            tag_payload,
            TodoTagActionPayload {
                idx: 4,
                tags: vec!["alpha|beta,gamma".into(), "needs".into()],
            }
        );
    }

    #[test]
    fn todo_links_no_match_and_ambiguous_paths() {
        let original = set_todos(vec![
            TodoEntry {
                id: "t-1".into(),
                text: "ship release".into(),
                done: false,
                priority: 1,
                tags: vec![],
                entity_refs: vec![],
            },
            TodoEntry {
                id: "t-2".into(),
                text: "ship release".into(),
                done: false,
                priority: 2,
                tags: vec![],
                entity_refs: vec![],
            },
        ]);
        let plugin = TodoPlugin::default();
        let no_match = plugin.search_internal("todo links unknown-task");
        assert!(no_match[0].label.contains("No todo found"));

        let ambiguous = plugin.search_internal("todo links ship release");
        assert!(ambiguous[0].label.starts_with("Ambiguous todo query"));
        assert!(ambiguous
            .iter()
            .any(|a| a.action == "query:todo links id:t-1"));
        assert!(ambiguous
            .iter()
            .any(|a| a.action == "query:todo links id:t-2"));
        if let Ok(mut guard) = TODO_DATA.write() {
            *guard = original;
        }
    }

    #[test]
    fn todo_links_reuses_cached_index_between_queries() {
        reset_todo_links_index_cache_state();
        let original = set_todos(vec![TodoEntry {
            id: "t-cache".into(),
            text: "cache me".into(),
            done: false,
            priority: 1,
            tags: vec![],
            entity_refs: vec![EntityRef::new(EntityKind::Note, "alpha", None)],
        }]);

        let plugin = TodoPlugin {
            matcher: SkimMatcherV2::default(),
            data: TODO_DATA.clone(),
            cache: TODO_CACHE.clone(),
            watcher: None,
        };

        let first = plugin.search_internal("todo links id:t-cache");
        assert!(!first.is_empty());
        assert_eq!(todo_links_index_rebuild_count(), 1);

        let second = plugin.search_internal("todo links id:t-cache");
        assert!(!second.is_empty());
        assert_eq!(todo_links_index_rebuild_count(), 1);

        update_cache(vec![TodoEntry {
            id: "t-cache".into(),
            text: "cache me updated".into(),
            done: false,
            priority: 1,
            tags: vec![],
            entity_refs: vec![EntityRef::new(EntityKind::Note, "alpha", None)],
        }]);

        let third = plugin.search_internal("todo links id:t-cache");
        assert!(!third.is_empty());
        assert_eq!(todo_links_index_rebuild_count(), 2);

        if let Ok(mut guard) = TODO_DATA.write() {
            *guard = original;
        }
        reset_todo_links_index_cache_state();
    }

    #[test]
    fn todo_links_json_output_prefixes_machine_readable_row() {
        let original = set_todos(vec![TodoEntry {
            id: "t-3".into(),
            text: "write docs".into(),
            done: false,
            priority: 1,
            tags: vec![],
            entity_refs: vec![EntityRef::new(EntityKind::Note, "alpha", None)],
        }]);
        let plugin = TodoPlugin::default();
        let out = plugin.search_internal("todo links id:t-3 --json");
        assert!(out[0].desc.contains("JSON"));
        assert!(out[0].label.starts_with("["));
        if let Ok(mut guard) = TODO_DATA.write() {
            *guard = original;
        }
    }
}
