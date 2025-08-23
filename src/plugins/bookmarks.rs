use crate::actions::Action;
use crate::plugin::Plugin;
use crate::common::lru::LruCache;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

pub const BOOKMARKS_FILE: &str = "bookmarks.json";

static BOOKMARK_CACHE: Lazy<Arc<Mutex<LruCache<String, Vec<Action>>>>> =
    Lazy::new(|| Arc::new(Mutex::new(LruCache::new(64))));

fn invalidate_bookmark_cache() {
    if let Ok(mut cache) = BOOKMARK_CACHE.lock() {
        cache.clear();
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BookmarkEntry {
    pub url: String,
    #[serde(default)]
    pub alias: Option<String>,
}

pub struct BookmarksPlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<Vec<BookmarkEntry>>>,
    cache: Arc<Mutex<LruCache<String, Vec<Action>>>>,
    #[allow(dead_code)]
    watcher: Option<RecommendedWatcher>,
}

impl BookmarksPlugin {
    /// Construct a new `BookmarksPlugin` with a fuzzy matcher.
    pub fn new() -> Self {
        let data = Arc::new(Mutex::new(
            load_bookmarks(BOOKMARKS_FILE).unwrap_or_default(),
        ));
        let cache = BOOKMARK_CACHE.clone();
        let data_clone = data.clone();
        let path = BOOKMARKS_FILE.to_string();
        let mut watcher = RecommendedWatcher::new(
            {
                let path = path.clone();
                let cache_clone = cache.clone();
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
                                if let Ok(mut c) = cache_clone.lock() {
                                    c.clear();
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
            cache,
            watcher,
        }
    }

    fn search_internal(&self, trimmed: &str) -> Vec<Action> {
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "bm") {
            if rest.trim().is_empty() || rest.trim().eq_ignore_ascii_case("add") {
                return vec![Action {
                    label: "bm: add bookmark".into(),
                    desc: "Bookmark".into(),
                    action: "bookmark:dialog".into(),
                    args: None,
                }];
            }
        }

        const ADD_PREFIX: &str = "bm add ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, ADD_PREFIX) {
            let url = rest.trim();
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
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, RM_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|b| {
                    self.matcher.fuzzy_match(&b.url, filter).is_some()
                        || b.alias
                            .as_ref()
                            .map(|a| self.matcher.fuzzy_match(a, filter).is_some())
                            .unwrap_or(false)
                })
                .map(|b| Action {
                    label: format!("Remove bookmark {}", b.url.clone()),
                    desc: "Bookmark".into(),
                    action: format!("bookmark:remove:{}", b.url.clone()),
                    args: None,
                })
                .collect();
        }
        const LIST_PREFIX: &str = "bm list";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, LIST_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
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
                        action: b.url.clone(),
                        args: None,
                    }
                })
                .collect();
        }
        const PREFIX: &str = "bm";
        let rest = match crate::common::strip_prefix_ci(trimmed, PREFIX) {
            Some(r) => r,
            None => return Vec::new(),
        };
        let filter = rest.trim();
        let guard = match self.data.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard
            .iter()
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
                    action: b.url.clone(),
                    args: None,
                }
            })
            .collect()
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
        invalidate_bookmark_cache();
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
        invalidate_bookmark_cache();
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
        invalidate_bookmark_cache();
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
        let key = trimmed.to_string();
        if let Ok(mut cache) = self.cache.lock() {
            if let Some(res) = cache.get(&key).cloned() {
                return res;
            }
        }

        let result = self.search_internal(trimmed);

        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(key, result.clone());
        }

        result
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
