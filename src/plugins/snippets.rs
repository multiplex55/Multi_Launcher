use crate::actions::Action;
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use crate::plugin::Plugin;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

pub const SNIPPETS_FILE: &str = "snippets.json";

static SNIPPETS_VERSION: AtomicU64 = AtomicU64::new(0);
static SNIPPETS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static VERSIONED_SNIPPETS: Lazy<Mutex<Option<(PathBuf, Vec<SnippetEntry>)>>> =
    Lazy::new(|| Mutex::new(None));

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SnippetEntry {
    pub alias: String,
    pub text: String,
}

/// Load all snippets from the JSON file at `path`.
pub fn load_snippets(path: &str) -> anyhow::Result<Vec<SnippetEntry>> {
    match load_snippets_typed(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Vec::new()),
        LoadState::Loaded(snippets) => Ok(snippets),
    }
}

pub fn load_snippets_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<SnippetEntry>>, PersistenceError> {
    load_json(path)
}

/// Persist `snippets` to `path`.
pub fn save_snippets(path: &str, snippets: &[SnippetEntry]) -> anyhow::Result<()> {
    replace_snippets(path, snippets.to_vec()).map(|_| ())
}

pub fn replace_snippets(
    path: &str,
    replacement: Vec<SnippetEntry>,
) -> anyhow::Result<Vec<SnippetEntry>> {
    update_snippets(path, move |snippets| {
        *snippets = replacement;
        Ok(true)
    })
}

pub fn update_snippets(
    path: &str,
    mutate: impl FnOnce(&mut Vec<SnippetEntry>) -> anyhow::Result<bool>,
) -> anyhow::Result<Vec<SnippetEntry>> {
    update_snippets_with_save(path, mutate, |path, snippets| {
        save_json_atomic(path, snippets).map_err(Into::into)
    })
}

fn update_snippets_with_save(
    path: &str,
    mutate: impl FnOnce(&mut Vec<SnippetEntry>) -> anyhow::Result<bool>,
    save: impl FnOnce(&str, &[SnippetEntry]) -> anyhow::Result<()>,
) -> anyhow::Result<Vec<SnippetEntry>> {
    let _transaction = snippets_transaction_guard();
    let mut snippets = load_snippets(path)?;
    if mutate(&mut snippets)? {
        save(path, &snippets)?;
        record_versioned_snippets(path, &snippets);
    }
    Ok(snippets)
}

fn snippets_transaction_guard() -> MutexGuard<'static, ()> {
    SNIPPETS_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Append or update a snippet entry identified by `alias`.
pub fn append_snippet(path: &str, alias: &str, text: &str) -> anyhow::Result<()> {
    let alias = alias.to_owned();
    let text = text.to_owned();
    update_snippets(path, move |list| {
        if let Some(item) = list.iter_mut().find(|entry| entry.alias == alias) {
            if item.text == text {
                return Ok(false);
            }
            item.text = text;
        } else {
            list.push(SnippetEntry { alias, text });
        }
        Ok(true)
    })?;
    Ok(())
}

/// Remove the snippet identified by `alias`.
pub fn remove_snippet(path: &str, alias: &str) -> anyhow::Result<()> {
    let alias = alias.to_owned();
    update_snippets(path, move |list| {
        let Some(pos) = list.iter().position(|entry| entry.alias == alias) else {
            return Ok(false);
        };
        list.remove(pos);
        Ok(true)
    })?;
    Ok(())
}

pub fn snippets_version() -> u64 {
    SNIPPETS_VERSION.load(Ordering::SeqCst)
}

fn bump_snippets_version() {
    SNIPPETS_VERSION.fetch_add(1, Ordering::SeqCst);
}

fn record_versioned_snippets(path: &str, snippets: &[SnippetEntry]) {
    *VERSIONED_SNIPPETS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) =
        Some((PathBuf::from(path), snippets.to_vec()));
    bump_snippets_version();
}

fn bump_for_external_snippets(path: &str, snippets: &[SnippetEntry]) {
    let mut versioned = VERSIONED_SNIPPETS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if versioned
        .as_ref()
        .is_some_and(|(saved_path, saved)| saved_path == Path::new(path) && saved == snippets)
    {
        return;
    }
    *versioned = Some((PathBuf::from(path), snippets.to_vec()));
    bump_snippets_version();
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
        let startup = load_snippets(SNIPPETS_FILE).unwrap_or_else(|error| {
            tracing::error!(%error, "snippet startup retained invalid persisted file");
            Vec::new()
        });
        let data = Arc::new(Mutex::new(startup.clone()));
        *VERSIONED_SNIPPETS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((PathBuf::from(SNIPPETS_FILE), startup));
        let data_clone = data.clone();
        let path = SNIPPETS_FILE.to_string();
        let mut watcher = RecommendedWatcher::new(
            {
                let path = path.clone();
                move |res: notify::Result<notify::Event>| {
                    if let Ok(event) = res
                        && matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        )
                    {
                        if let Err(error) = reload_snippet_snapshot(&path, &data_clone) {
                            tracing::error!(%error, "invalid snippet reload retained last-good state");
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

fn reload_snippet_snapshot(path: &str, data: &Arc<Mutex<Vec<SnippetEntry>>>) -> anyhow::Result<()> {
    let _transaction = snippets_transaction_guard();
    let snippets = load_snippets(path)?;
    let mut current = data
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *current == snippets {
        return Ok(());
    }
    *current = snippets.clone();
    drop(current);
    bump_for_external_snippets(path, &snippets);
    Ok(())
}

impl Default for SnippetsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for SnippetsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "cs")
            && rest.is_empty()
        {
            return vec![Action {
                label: "cs: edit snippets".into(),
                desc: "Snippet".into(),
                action: "snippet:dialog".into(),
                args: None,
            }];
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
            let rest = rest.trim();
            if let Some((alias, text)) = rest.split_once(' ') {
                let alias = alias.trim();
                let text = text.trim();
                if !alias.is_empty() && !text.is_empty() {
                    return vec![Action {
                        label: format!("Edit snippet {alias}"),
                        desc: "Snippet".into(),
                        action: format!("snippet:add:{alias}|{text}"),
                        args: None,
                    }];
                }
            }
            let filter = rest;
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

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use std::sync::Barrier;

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn snippet(alias: &str, text: &str) -> SnippetEntry {
        SnippetEntry {
            alias: alias.into(),
            text: text.into(),
        }
    }

    #[test]
    fn typed_load_and_pretty_schema_cover_all_file_states() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert_eq!(load_snippets_typed(&missing).unwrap(), LoadState::Missing);
        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n\t").unwrap();
        assert_eq!(load_snippets_typed(&empty).unwrap(), LoadState::Empty);
        let expected = vec![snippet("multi", "first\nsecond")];
        let valid = directory.path().join("valid.json");
        std::fs::write(&valid, serde_json::to_vec(&expected).unwrap()).unwrap();
        assert_eq!(
            load_snippets_typed(&valid).unwrap(),
            LoadState::Loaded(expected.clone())
        );
        let saved = directory.path().join("saved.json");
        save_snippets(saved.to_str().unwrap(), &expected).unwrap();
        assert_eq!(
            std::fs::read_to_string(saved).unwrap(),
            serde_json::to_string_pretty(&expected).unwrap()
        );
        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "{broken").unwrap();
        assert!(matches!(
            load_snippets_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
        assert!(matches!(
            load_snippets_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
    }

    #[test]
    fn malformed_file_rejects_add_edit_remove_and_replacement_unchanged() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let invalid = b"not snippets JSON";
        std::fs::write(&path, invalid).unwrap();
        let path = path.to_str().unwrap();
        for result in [
            append_snippet(path, "new", "text"),
            append_snippet(path, "existing", "edited"),
            remove_snippet(path, "existing"),
            save_snippets(path, &[snippet("replacement", "lost")]),
        ] {
            assert!(result.is_err());
            assert_eq!(std::fs::read(path).unwrap(), invalid);
        }
    }

    #[test]
    fn concurrent_mutations_both_survive() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = Arc::new(
            directory
                .path()
                .join("snippets.json")
                .to_string_lossy()
                .into_owned(),
        );
        let barrier = Arc::new(Barrier::new(3));
        let first = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                append_snippet(&path, "first", "one").unwrap();
            })
        };
        let second = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                append_snippet(&path, "second", "two").unwrap();
            })
        };
        barrier.wait();
        first.join().unwrap();
        second.join().unwrap();
        let committed = load_snippets(&path).unwrap();
        assert!(committed.contains(&snippet("first", "one")));
        assert!(committed.contains(&snippet("second", "two")));
    }

    #[test]
    fn failed_save_retains_destination_and_version() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let original = vec![snippet("saved", "value")];
        std::fs::write(&path, serde_json::to_vec_pretty(&original).unwrap()).unwrap();
        let version = snippets_version();
        let result = update_snippets_with_save(
            path.to_str().unwrap(),
            |snippets| {
                snippets.push(snippet("lost", "value"));
                Ok(true)
            },
            |_path, _snippets| anyhow::bail!("deterministic replacement failure"),
        );
        assert!(result.is_err());
        assert_eq!(load_snippets(path.to_str().unwrap()).unwrap(), original);
        assert_eq!(snippets_version(), version);
    }

    #[test]
    fn watcher_retains_invalid_recovers_and_does_not_double_bump_local_save() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let path_text = path.to_str().unwrap();
        let initial = vec![snippet("initial", "value")];
        std::fs::write(&path, serde_json::to_vec_pretty(&initial).unwrap()).unwrap();
        let data = Arc::new(Mutex::new(initial.clone()));
        let local = vec![snippet("local", "value")];
        save_snippets(path_text, &local).unwrap();
        let after_local = snippets_version();
        reload_snippet_snapshot(path_text, &data).unwrap();
        assert_eq!(*data.lock().unwrap(), local);
        assert_eq!(snippets_version(), after_local);

        std::fs::write(&path, "invalid").unwrap();
        assert!(reload_snippet_snapshot(path_text, &data).is_err());
        assert_eq!(*data.lock().unwrap(), local);
        assert_eq!(snippets_version(), after_local);

        let external = vec![snippet("external", "value")];
        std::fs::write(&path, serde_json::to_vec_pretty(&external).unwrap()).unwrap();
        reload_snippet_snapshot(path_text, &data).unwrap();
        assert_eq!(*data.lock().unwrap(), external);
        assert_eq!(snippets_version(), after_local + 1);
    }
}
