use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const SNIPPETS_FILE: &str = "snippets.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct SnippetEntry {
    pub alias: String,
    pub text: String,
}

/// Load all snippets from the JSON file at `path`.
pub fn load_snippets(path: &str) -> anyhow::Result<Vec<SnippetEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<SnippetEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

/// Persist `snippets` to `path`.
pub fn save_snippets(path: &str, snippets: &[SnippetEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(snippets)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Append or update a snippet entry identified by `alias`.
pub fn append_snippet(path: &str, alias: &str, text: &str) -> anyhow::Result<()> {
    let mut list = load_snippets(path).unwrap_or_default();
    if let Some(item) = list.iter_mut().find(|e| e.alias == alias) {
        item.text = text.to_string();
    } else {
        list.push(SnippetEntry {
            alias: alias.to_string(),
            text: text.to_string(),
        });
    }
    save_snippets(path, &list)
}

/// Remove the snippet identified by `alias`.
pub fn remove_snippet(path: &str, alias: &str) -> anyhow::Result<()> {
    let mut list = load_snippets(path).unwrap_or_default();
    if let Some(pos) = list.iter().position(|e| e.alias == alias) {
        list.remove(pos);
        save_snippets(path, &list)?;
    }
    Ok(())
}

pub struct SnippetsPlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<Vec<SnippetEntry>>>,
    #[allow(dead_code)]
    watcher: Option<RecommendedWatcher>,
}

impl SnippetsPlugin {
    /// Create a new snippets plugin instance.
    pub fn new() -> Self {
        let data = Arc::new(Mutex::new(load_snippets(SNIPPETS_FILE).unwrap_or_default()));
        let data_clone = data.clone();
        let path = SNIPPETS_FILE.to_string();
        let mut watcher = RecommendedWatcher::new(
            {
                let path = path.clone();
                move |res: notify::Result<notify::Event>| {
                    if let Ok(event) = res {
                        if matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        ) {
                            if let Ok(list) = load_snippets(&path) {
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

impl Default for SnippetsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for SnippetsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cs") {
            if rest.is_empty() {
                return vec![Action {
                    label: "cs: edit snippets".into(),
                    desc: "Snippet".into(),
                    action: "snippet:dialog".into(),
                    args: None,
                }];
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cs rm") {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|s| {
                    filter.is_empty()
                        || self.matcher.fuzzy_match(&s.alias, filter).is_some()
                        || self.matcher.fuzzy_match(&s.text, filter).is_some()
                })
                .map(|s| Action {
                    label: format!("Remove snippet {}", s.alias.clone()),
                    desc: "Snippet".into(),
                    action: format!("snippet:remove:{}", s.alias.clone()),
                    args: None,
                })
                .collect();
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cs add ") {
            let mut parts = rest.trim().splitn(2, ' ');
            let alias = parts.next().unwrap_or("").trim();
            let text = parts.next().unwrap_or("").trim();
            if !alias.is_empty() && !text.is_empty() {
                return vec![Action {
                    label: format!("Add snippet {alias}"),
                    desc: "Snippet".into(),
                    action: format!("snippet:add:{alias}|{text}"),
                    args: None,
                }];
            }
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cs edit") {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|s| {
                    filter.is_empty()
                        || self.matcher.fuzzy_match(&s.alias, filter).is_some()
                        || self.matcher.fuzzy_match(&s.text, filter).is_some()
                })
                .map(|s| Action {
                    label: format!("Edit snippet {}", s.alias.clone()),
                    desc: "Snippet".into(),
                    action: format!("snippet:edit:{}", s.alias.clone()),
                    args: None,
                })
                .collect();
        }

        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cs list") {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|s| {
                    self.matcher.fuzzy_match(&s.alias, filter).is_some()
                        || self.matcher.fuzzy_match(&s.text, filter).is_some()
                })
                .map(|s| Action {
                    label: s.alias.clone(),
                    desc: "Snippet".into(),
                    action: format!("clipboard:{}", s.text.clone()),
                    args: None,
                })
                .collect();
        }

        if let Some(filter) = crate::common::strip_prefix_ci(trimmed, "cs") {
            let filter = filter.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|s| {
                    self.matcher.fuzzy_match(&s.alias, filter).is_some()
                        || self.matcher.fuzzy_match(&s.text, filter).is_some()
                })
                .map(|s| Action {
                    label: s.alias.clone(),
                    desc: "Snippet".into(),
                    action: format!("clipboard:{}", s.text.clone()),
                    args: None,
                })
                .collect();
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "snippets"
    }

    fn description(&self) -> &str {
        "Search saved text snippets (prefix: `cs`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "cs".into(),
                desc: "Snippet".into(),
                action: "query:cs".into(),
                args: None,
            },
            Action {
                label: "cs add".into(),
                desc: "Snippet".into(),
                action: "query:cs add ".into(),
                args: None,
            },
            Action {
                label: "cs rm".into(),
                desc: "Snippet".into(),
                action: "query:cs rm ".into(),
                args: None,
            },
            Action {
                label: "cs list".into(),
                desc: "Snippet".into(),
                action: "query:cs list".into(),
                args: None,
            },
            Action {
                label: "cs edit".into(),
                desc: "Snippet".into(),
                action: "query:cs edit".into(),
                args: None,
            },
        ]
    }
}
