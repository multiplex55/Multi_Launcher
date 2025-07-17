use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const BOOKMARKS_FILE: &str = "bookmarks.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct BookmarkEntry {
    pub url: String,
    #[serde(default)]
    pub alias: Option<String>,
}

pub struct BookmarksPlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<Vec<BookmarkEntry>>>,
    #[allow(dead_code)]
    watcher: Option<RecommendedWatcher>,
}

impl BookmarksPlugin {
    /// Construct a new `BookmarksPlugin` with a fuzzy matcher.
    pub fn new() -> Self {
        let data = Arc::new(Mutex::new(
            load_bookmarks(BOOKMARKS_FILE).unwrap_or_default(),
        ));
        let data_clone = data.clone();
        let path = BOOKMARKS_FILE.to_string();
        let mut watcher = RecommendedWatcher::new(
            {
                let path = path.clone();
                move |res: notify::Result<notify::Event>| {
                    if let Ok(event) = res {
                        if matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        ) {
                            if let Ok(list) = load_bookmarks(&path) {
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

fn normalize_url(url: &str) -> String {
    let mut out = url.trim().to_string();
    if out.starts_with("http://") {
        out = out.replacen("http://", "https://", 1);
    } else if !out.starts_with("https://") {
        out = format!("https://{out}");
    }
    if let Some(rest) = out.strip_prefix("https://") {
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        if !host.starts_with("www.") && !host.contains('.') {
            let host = format!("www.{host}");
            out = if path.is_empty() {
                format!("https://{host}")
            } else {
                format!("https://{host}/{path}")
            };
        }
    }
    out
}

/// Load bookmarks from `path`.
///
/// Returns an empty list if the file does not exist or is empty.
pub fn load_bookmarks(path: &str) -> anyhow::Result<Vec<BookmarkEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    // Try new format first, fall back to plain string list for backward compatibility
    let list: Result<Vec<BookmarkEntry>, _> = serde_json::from_str(&content);
    match list {
        Ok(items) => Ok(items),
        Err(_) => {
            let list: Vec<String> = serde_json::from_str(&content)?;
            Ok(list
                .into_iter()
                .map(|url| BookmarkEntry { url, alias: None })
                .collect())
        }
    }
}

/// Save the provided `bookmarks` to `path` in JSON format.
pub fn save_bookmarks(path: &str, bookmarks: &[BookmarkEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(bookmarks)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Append a new bookmark `url` to the file at `path`.
///
/// The URL is normalized and duplicates are ignored.
pub fn append_bookmark(path: &str, url: &str) -> anyhow::Result<()> {
    let mut list = load_bookmarks(path).unwrap_or_default();
    let fixed = normalize_url(url);
    if !list.iter().any(|b| b.url == fixed) {
        list.push(BookmarkEntry {
            url: fixed,
            alias: None,
        });
        save_bookmarks(path, &list)?;
    }
    Ok(())
}

/// Remove the bookmark matching `url` from the file at `path`.
pub fn remove_bookmark(path: &str, url: &str) -> anyhow::Result<()> {
    let mut list = load_bookmarks(path).unwrap_or_default();
    let fixed = normalize_url(url);
    if let Some(pos) = list.iter().position(|b| b.url == fixed) {
        list.remove(pos);
        save_bookmarks(path, &list)?;
    }
    Ok(())
}

/// Set or clear the alias of a bookmark.
///
/// Passing an empty `alias` removes the existing alias.
pub fn set_alias(path: &str, url: &str, alias: &str) -> anyhow::Result<()> {
    let mut list = load_bookmarks(path).unwrap_or_default();
    let fixed = normalize_url(url);
    if let Some(item) = list.iter_mut().find(|b| b.url == fixed) {
        item.alias = if alias.is_empty() {
            None
        } else {
            Some(alias.to_string())
        };
        save_bookmarks(path, &list)?;
    }
    Ok(())
}

impl Default for BookmarksPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for BookmarksPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("bm") || trimmed.eq_ignore_ascii_case("bm add") {
            return vec![Action {
                label: "bm: add bookmark".into(),
                desc: "Bookmark".into(),
                action: "bookmark:dialog".into(),
                args: None,
            }];
        }

        const ADD_PREFIX: &str = "bm add ";
        if trimmed.len() >= ADD_PREFIX.len()
            && trimmed[..ADD_PREFIX.len()].eq_ignore_ascii_case(ADD_PREFIX)
        {
            let url = trimmed[ADD_PREFIX.len()..].trim();
            if !url.is_empty() {
                let norm = normalize_url(url);
                return vec![Action {
                    label: format!("Add bookmark {norm}"),
                    desc: "Bookmark".into(),
                    action: format!("bookmark:add:{norm}"),
                    args: None,
                }];
            }
        }
        const RM_PREFIX: &str = "bm rm";
        if trimmed.len() >= RM_PREFIX.len()
            && trimmed[..RM_PREFIX.len()].eq_ignore_ascii_case(RM_PREFIX)
        {
            let rest = &trimmed[RM_PREFIX.len()..];
            let filter = rest.trim();
            let bookmarks = self.data.lock().unwrap().clone();
            return bookmarks
                .into_iter()
                .filter(|b| {
                    self.matcher.fuzzy_match(&b.url, filter).is_some()
                        || b.alias
                            .as_ref()
                            .map(|a| self.matcher.fuzzy_match(a, filter).is_some())
                            .unwrap_or(false)
                })
                .map(|b| Action {
                    label: format!("Remove bookmark {}", b.url),
                    desc: "Bookmark".into(),
                    action: format!("bookmark:remove:{}", b.url),
                    args: None,
                })
                .collect();
        }
        const LIST_PREFIX: &str = "bm list";
        if trimmed.len() >= LIST_PREFIX.len()
            && trimmed[..LIST_PREFIX.len()].eq_ignore_ascii_case(LIST_PREFIX)
        {
            let rest = &trimmed[LIST_PREFIX.len()..];
            let filter = rest.trim();
            let bookmarks = self.data.lock().unwrap().clone();
            return bookmarks
                .into_iter()
                .filter(|b| {
                    self.matcher.fuzzy_match(&b.url, filter).is_some()
                        || b.alias
                            .as_ref()
                            .map(|a| self.matcher.fuzzy_match(a, filter).is_some())
                            .unwrap_or(false)
                })
                .map(|b| {
                    let label = b.alias.clone().unwrap_or_else(|| b.url.clone());
                    Action {
                        label,
                        desc: "Bookmark".into(),
                        action: b.url,
                        args: None,
                    }
                })
                .collect();
        }
        const PREFIX: &str = "bm";
        if trimmed.len() < PREFIX.len() || !trimmed[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
            return Vec::new();
        }
        let filter = trimmed[PREFIX.len()..].trim();
        let bookmarks = self.data.lock().unwrap().clone();
        bookmarks
            .into_iter()
            .filter(|b| {
                self.matcher.fuzzy_match(&b.url, filter).is_some()
                    || b.alias
                        .as_ref()
                        .map(|a| self.matcher.fuzzy_match(a, filter).is_some())
                        .unwrap_or(false)
            })
            .map(|b| {
                let label = b.alias.clone().unwrap_or_else(|| b.url.clone());
                Action {
                    label,
                    desc: "Bookmark".into(),
                    action: b.url,
                    args: None,
                }
            })
            .collect()
    }

    fn name(&self) -> &str {
        "bookmarks"
    }

    fn description(&self) -> &str {
        "Return bookmarked URLs (prefix: `bm`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "bm".into(),
                desc: "Bookmark".into(),
                action: "query:bm ".into(),
                args: None,
            },
            Action {
                label: "bm add".into(),
                desc: "Bookmark".into(),
                action: "query:bm add ".into(),
                args: None,
            },
            Action {
                label: "bm rm".into(),
                desc: "Bookmark".into(),
                action: "query:bm rm ".into(),
                args: None,
            },
            Action {
                label: "bm list".into(),
                desc: "Bookmark".into(),
                action: "query:bm list".into(),
                args: None,
            },
        ]
    }
}
