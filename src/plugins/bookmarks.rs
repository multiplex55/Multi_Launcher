use crate::actions::Action;
use crate::common::json_watch::{JsonWatcher, watch_json};
use crate::common::lru::LruCache;
use crate::common::persistence::{LoadState, PersistenceError, read_bytes, save_json_atomic};
use crate::plugin::Plugin;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

pub const BOOKMARKS_FILE: &str = "bookmarks.json";

static BOOKMARK_CACHE: Lazy<Arc<Mutex<LruCache<String, Vec<Action>>>>> =
    Lazy::new(|| Arc::new(Mutex::new(LruCache::new(64))));
static BOOKMARKS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static LIVE_BOOKMARKS: Lazy<super::live_snapshot::LiveSnapshotRegistry<BookmarkEntry>> =
    Lazy::new(super::live_snapshot::LiveSnapshotRegistry::new);

fn invalidate_bookmark_cache() {
    if let Ok(mut cache) = BOOKMARK_CACHE.lock() {
        cache.clear();
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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
    watcher: Option<JsonWatcher>,
}

impl BookmarksPlugin {
    /// Construct a new `BookmarksPlugin` with a fuzzy matcher.
    pub fn new() -> Self {
        Self::new_for_path(BOOKMARKS_FILE)
    }

    fn new_for_path(path: &str) -> Self {
        let data = {
            let _transaction = bookmarks_transaction_guard();
            let startup = match load_bookmarks(path) {
                Ok(bookmarks) => Some(bookmarks),
                Err(error) => {
                    tracing::error!(%error, "bookmark startup retained invalid persisted file");
                    None
                }
            };
            LIVE_BOOKMARKS.get_or_create(path, startup)
        };
        let cache = BOOKMARK_CACHE.clone();
        let data_clone = data.clone();
        let path = path.to_string();
        let watcher = watch_json(&path, {
            let path = path.clone();
            let cache_clone = cache.clone();
            move || {
                if let Err(error) = reload_bookmark_snapshot(&path, &data_clone, &cache_clone) {
                    tracing::error!(%error, "invalid bookmark reload retained last-good state");
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
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "bm")
            && (rest.trim().is_empty() || rest.trim().eq_ignore_ascii_case("add"))
        {
            return vec![Action {
                label: "bm: add bookmark".into(),
                desc: "Bookmark".into(),
                action: "bookmark:dialog".into(),
                args: None,
            }];
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
    match load_bookmarks_typed(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(bookmarks) => Ok(bookmarks),
    }
}

pub fn load_bookmarks_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<BookmarkEntry>>, PersistenceError> {
    let path = path.as_ref();
    match read_bytes(path)? {
        LoadState::Missing => Ok(LoadState::Missing),
        LoadState::Empty => Ok(LoadState::Empty),
        LoadState::Loaded(bytes) => {
            if let Ok(bookmarks) = serde_json::from_slice::<Vec<BookmarkEntry>>(&bytes) {
                return Ok(LoadState::Loaded(bookmarks));
            }
            serde_json::from_slice::<Vec<String>>(&bytes)
                .map(|legacy| {
                    LoadState::Loaded(
                        legacy
                            .into_iter()
                            .map(|url| BookmarkEntry { url, alias: None })
                            .collect(),
                    )
                })
                .map_err(|source| PersistenceError::MalformedJson {
                    path: path.to_path_buf(),
                    source,
                })
        }
    }
}

/// Save the provided `bookmarks` to `path` in JSON format.
pub fn save_bookmarks(path: &str, bookmarks: &[BookmarkEntry]) -> anyhow::Result<()> {
    let replacement = bookmarks.to_vec();
    update_bookmarks(path, move |current| {
        *current = replacement;
        Ok(true)
    })
    .map(|_| ())
}

pub fn update_bookmarks(
    path: &str,
    mutate: impl FnOnce(&mut Vec<BookmarkEntry>) -> anyhow::Result<bool>,
) -> anyhow::Result<Vec<BookmarkEntry>> {
    update_bookmarks_with_save(path, mutate, |path, bookmarks| {
        save_json_atomic(path, bookmarks).map_err(Into::into)
    })
}

fn update_bookmarks_with_save(
    path: &str,
    mutate: impl FnOnce(&mut Vec<BookmarkEntry>) -> anyhow::Result<bool>,
    save: impl FnOnce(&str, &[BookmarkEntry]) -> anyhow::Result<()>,
) -> anyhow::Result<Vec<BookmarkEntry>> {
    let _transaction = bookmarks_transaction_guard();
    let mut bookmarks = load_bookmarks(path)?;
    if mutate(&mut bookmarks)? {
        save(path, &bookmarks)?;
        LIVE_BOOKMARKS.publish(path, &bookmarks);
        invalidate_bookmark_cache();
    }
    Ok(bookmarks)
}

fn bookmarks_transaction_guard() -> MutexGuard<'static, ()> {
    BOOKMARKS_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Append a new bookmark `url` to the file at `path`.
///
/// The URL is normalized and duplicates are ignored when no alias update was requested.
pub fn append_bookmark(path: &str, url: &str) -> anyhow::Result<()> {
    append_bookmark_with_alias(path, url, None)
}

pub fn append_bookmark_with_alias(
    path: &str,
    url: &str,
    alias: Option<&str>,
) -> anyhow::Result<()> {
    let fixed = normalize_url(url);
    let requested_alias = alias.map(|value| {
        if value.is_empty() {
            None
        } else {
            Some(value.to_owned())
        }
    });
    update_bookmarks(path, move |list| {
        if let Some(bookmark) = list.iter_mut().find(|bookmark| bookmark.url == fixed) {
            let Some(alias) = requested_alias.clone() else {
                return Ok(false);
            };
            if bookmark.alias == alias {
                return Ok(false);
            }
            bookmark.alias = alias;
            return Ok(true);
        }
        list.push(BookmarkEntry {
            url: fixed,
            alias: requested_alias.flatten(),
        });
        Ok(true)
    })?;
    Ok(())
}

/// Remove the bookmark matching `url` from the file at `path`.
pub fn remove_bookmark(path: &str, url: &str) -> anyhow::Result<()> {
    let fixed = normalize_url(url);
    update_bookmarks(path, move |list| {
        let Some(pos) = list.iter().position(|bookmark| bookmark.url == fixed) else {
            return Ok(false);
        };
        list.remove(pos);
        Ok(true)
    })?;
    Ok(())
}

/// Set or clear the alias of a bookmark.
///
/// Passing an empty `alias` removes the existing alias.
pub fn set_alias(path: &str, url: &str, alias: &str) -> anyhow::Result<()> {
    let fixed = normalize_url(url);
    let alias = alias.to_owned();
    update_bookmarks(path, move |list| {
        let Some(item) = list.iter_mut().find(|bookmark| bookmark.url == fixed) else {
            return Ok(false);
        };
        let updated = if alias.is_empty() { None } else { Some(alias) };
        if item.alias == updated {
            return Ok(false);
        }
        item.alias = updated;
        Ok(true)
    })?;
    Ok(())
}

fn reload_bookmark_snapshot(
    path: &str,
    data: &Arc<Mutex<Vec<BookmarkEntry>>>,
    cache: &Arc<Mutex<LruCache<String, Vec<Action>>>>,
) -> anyhow::Result<()> {
    let _transaction = bookmarks_transaction_guard();
    let bookmarks = match load_bookmarks_typed(path)? {
        LoadState::Missing => {
            anyhow::bail!("bookmarks file was removed; retaining last-good state")
        }
        LoadState::Empty => Vec::new(),
        LoadState::Loaded(bookmarks) => bookmarks,
    };
    if let Ok(mut current) = data.lock() {
        if *current == bookmarks {
            return Ok(());
        }
        *current = bookmarks;
    }
    if let Ok(mut cache) = cache.lock() {
        cache.clear();
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
        if let Ok(mut cache) = self.cache.lock()
            && let Some(res) = cache.get(&key).cloned()
        {
            return res;
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

    fn query_prefixes(&self) -> &[&str] {
        &["bm"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::time::{Duration, Instant};

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn bookmark(url: &str, alias: Option<&str>) -> BookmarkEntry {
        BookmarkEntry {
            url: url.into(),
            alias: alias.map(str::to_owned),
        }
    }

    #[test]
    fn typed_load_preserves_missing_empty_structured_legacy_and_errors() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert_eq!(load_bookmarks_typed(&missing).unwrap(), LoadState::Missing);

        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n\t").unwrap();
        assert_eq!(load_bookmarks_typed(&empty).unwrap(), LoadState::Empty);

        let structured = directory.path().join("structured.json");
        let expected = vec![bookmark("https://example.com", Some("Example"))];
        std::fs::write(&structured, serde_json::to_vec(&expected).unwrap()).unwrap();
        assert_eq!(
            load_bookmarks_typed(&structured).unwrap(),
            LoadState::Loaded(expected.clone())
        );
        let saved = directory.path().join("saved.json");
        save_bookmarks(saved.to_str().unwrap(), &expected).unwrap();
        assert_eq!(
            std::fs::read_to_string(saved).unwrap(),
            serde_json::to_string_pretty(&expected).unwrap()
        );

        let legacy = directory.path().join("legacy.json");
        std::fs::write(&legacy, br#"["https://legacy.example"]"#).unwrap();
        assert_eq!(
            load_bookmarks_typed(&legacy).unwrap(),
            LoadState::Loaded(vec![bookmark("https://legacy.example", None)])
        );

        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "{broken").unwrap();
        assert!(matches!(
            load_bookmarks_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
        assert!(matches!(
            load_bookmarks_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
    }

    #[test]
    fn malformed_file_rejects_add_remove_and_alias_without_changing_bytes() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bookmarks.json");
        let invalid = b"not bookmark JSON";
        std::fs::write(&path, invalid).unwrap();
        let path = path.to_str().unwrap();

        assert!(append_bookmark(path, "example.com").is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        assert!(remove_bookmark(path, "example.com").is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        assert!(set_alias(path, "example.com", "alias").is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }

    #[test]
    fn concurrent_adds_both_survive() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = Arc::new(
            directory
                .path()
                .join("bookmarks.json")
                .to_string_lossy()
                .into_owned(),
        );
        let barrier = Arc::new(Barrier::new(3));
        let first = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                append_bookmark(&path, "first.example").unwrap();
            })
        };
        let second = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                append_bookmark(&path, "second.example").unwrap();
            })
        };

        barrier.wait();
        first.join().unwrap();
        second.join().unwrap();
        let committed = load_bookmarks(&path).unwrap();
        assert_eq!(committed.len(), 2);
        assert!(committed.iter().any(|entry| entry.url.contains("first")));
        assert!(committed.iter().any(|entry| entry.url.contains("second")));
    }

    #[test]
    fn add_with_alias_updates_existing_entry_in_one_transaction() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bookmarks.json");
        let path = path.to_str().unwrap();

        append_bookmark_with_alias(path, "example.com", Some("first")).unwrap();
        append_bookmark_with_alias(path, "example.com", Some("updated")).unwrap();

        assert_eq!(
            load_bookmarks(path).unwrap(),
            vec![bookmark("https://example.com", Some("updated"))]
        );
    }

    #[test]
    fn failed_save_retains_destination_and_cache() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bookmarks.json");
        let original = vec![bookmark("https://committed.example", None)];
        std::fs::write(&path, serde_json::to_vec_pretty(&original).unwrap()).unwrap();
        let key = "cached".to_string();
        let cached = vec![Action {
            label: "cached".into(),
            desc: String::new(),
            action: "cached:action".into(),
            args: None,
        }];
        BOOKMARK_CACHE
            .lock()
            .unwrap()
            .insert(key.clone(), cached.clone());

        let result = update_bookmarks_with_save(
            path.to_str().unwrap(),
            |bookmarks| {
                bookmarks.push(bookmark("https://uncommitted.example", None));
                Ok(true)
            },
            |_path, _bookmarks| anyhow::bail!("deterministic save failure"),
        );

        assert!(result.is_err());
        assert_eq!(load_bookmarks(path.to_str().unwrap()).unwrap(), original);
        assert_eq!(
            BOOKMARK_CACHE.lock().unwrap().get(&key).cloned(),
            Some(cached)
        );
    }

    #[test]
    fn invalid_reload_retains_last_good_then_valid_reload_recovers() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bookmarks.json");
        let initial = vec![bookmark("https://initial.example", None)];
        let data = Arc::new(Mutex::new(initial.clone()));
        let cache = Arc::new(Mutex::new(LruCache::new(4)));
        std::fs::write(&path, "invalid").unwrap();

        assert!(reload_bookmark_snapshot(path.to_str().unwrap(), &data, &cache).is_err());
        assert_eq!(*data.lock().unwrap(), initial);

        std::fs::remove_file(&path).unwrap();
        assert!(reload_bookmark_snapshot(path.to_str().unwrap(), &data, &cache).is_err());
        assert_eq!(*data.lock().unwrap(), initial);

        let recovered = vec![bookmark("https://recovered.example", Some("Recovered"))];
        std::fs::write(&path, serde_json::to_vec_pretty(&recovered).unwrap()).unwrap();
        reload_bookmark_snapshot(path.to_str().unwrap(), &data, &cache).unwrap();
        assert_eq!(*data.lock().unwrap(), recovered);
    }

    #[test]
    fn committed_mutation_is_visible_to_all_instances_without_watcher_delivery() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bookmarks.json");
        let path = path.to_str().unwrap();
        save_bookmarks(path, &[]).unwrap();
        let first = BookmarksPlugin::new_for_path(path);
        let second = BookmarksPlugin::new_for_path(path);

        append_bookmark_with_alias(path, "example.com", Some("Immediate")).unwrap();

        for plugin in [&first, &second] {
            assert!(plugin.search("bm Immediate").iter().any(|action| {
                action.action == "https://example.com" && action.label == "Immediate"
            }));
        }
    }

    #[test]
    fn native_watcher_retains_invalid_and_removed_then_recovers_valid_snapshot() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bookmarks.json");
        let path_text = path.to_str().unwrap();
        let initial = vec![bookmark("https://initial.example", Some("Initial"))];
        save_bookmarks(path_text, &initial).unwrap();
        let plugin = BookmarksPlugin::new_for_path(path_text);
        let (notify_tx, notify_rx) = std::sync::mpsc::channel();
        let _probe = watch_json(&path, move || {
            let _ = notify_tx.send(());
        })
        .unwrap();

        std::fs::write(&path, "invalid").unwrap();
        notify_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("malformed replacement should notify");
        assert!(
            plugin
                .search("bm Initial")
                .iter()
                .any(|a| a.label == "Initial")
        );

        while notify_rx.try_recv().is_ok() {}
        std::fs::remove_file(&path).unwrap();
        notify_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("removal should notify");
        assert!(
            plugin
                .search("bm Initial")
                .iter()
                .any(|a| a.label == "Initial")
        );

        while notify_rx.try_recv().is_ok() {}
        let recovered = vec![bookmark("https://recovered.example", Some("Recovered"))];
        crate::common::persistence::save_json_atomic(&path, &recovered).unwrap();
        notify_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("valid atomic replacement should notify");
        let deadline = Instant::now() + Duration::from_secs(3);
        while !plugin
            .search("bm Recovered")
            .iter()
            .any(|action| action.label == "Recovered")
        {
            assert!(
                Instant::now() < deadline,
                "valid watcher reload did not publish"
            );
            std::thread::yield_now();
        }
    }
}
