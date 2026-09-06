use crate::actions::Action;
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use crate::plugin::Plugin;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

pub const FOLDERS_FILE: &str = "folders.json";
static FOLDERS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    pub label: String,
    pub path: String,
    #[serde(default)]
    pub alias: Option<String>,
}

/// Return a set of default commonly used folders.
pub fn default_folders() -> Vec<FolderEntry> {
    let mut out = Vec::new();
    if let Some(p) = dirs_next::home_dir() {
        out.push(FolderEntry {
            label: "Home".into(),
            path: p.to_string_lossy().into(),
            alias: None,
        });
    }
    if let Some(p) = dirs_next::download_dir() {
        out.push(FolderEntry {
            label: "Downloads".into(),
            path: p.to_string_lossy().into(),
            alias: None,
        });
    }
    if let Some(p) = dirs_next::desktop_dir() {
        out.push(FolderEntry {
            label: "Desktop".into(),
            path: p.to_string_lossy().into(),
            alias: None,
        });
    }
    if let Some(p) = dirs_next::document_dir() {
        out.push(FolderEntry {
            label: "Documents".into(),
            path: p.to_string_lossy().into(),
            alias: None,
        });
    }
    out
}

/// Load folder entries from `path` or return the defaults if the file is empty.
pub fn load_folders(path: &str) -> anyhow::Result<Vec<FolderEntry>> {
    match load_folders_typed(path)? {
        LoadState::Missing | LoadState::Empty => Ok(default_folders()),
        LoadState::Loaded(folders) => Ok(folders),
    }
}

pub fn load_folders_typed(
    path: impl AsRef<Path>,
) -> Result<LoadState<Vec<FolderEntry>>, PersistenceError> {
    load_json(path)
}

/// Save `folders` to `path` in JSON format.
pub fn save_folders(path: &str, folders: &[FolderEntry]) -> anyhow::Result<()> {
    let replacement = folders.to_vec();
    update_folders(path, move |current| {
        *current = replacement;
        Ok(true)
    })
    .map(|_| ())
}

pub fn update_folders(
    path: &str,
    mutate: impl FnOnce(&mut Vec<FolderEntry>) -> anyhow::Result<bool>,
) -> anyhow::Result<Vec<FolderEntry>> {
    let _transaction = folders_transaction_guard();
    let mut folders = load_folders(path)?;
    if mutate(&mut folders)? {
        save_json_atomic(path, &folders)?;
    }
    Ok(folders)
}

fn folders_transaction_guard() -> MutexGuard<'static, ()> {
    FOLDERS_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Append a folder path to the list stored at `path`.
///
/// Returns an error if the folder does not exist.
pub fn append_folder(path: &str, folder: &str) -> anyhow::Result<()> {
    if !std::path::Path::new(folder).exists() {
        anyhow::bail!("folder does not exist: {folder}");
    }

    let folder = folder.to_owned();
    update_folders(path, move |list| {
        if list.iter().any(|entry| entry.path == folder) {
            return Ok(false);
        }
        let label = std::path::Path::new(&folder)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| folder.clone());
        list.push(FolderEntry {
            label,
            path: folder,
            alias: None,
        });
        Ok(true)
    })?;
    Ok(())
}

/// Remove a folder entry matching `folder` from the file at `path`.
pub fn remove_folder(path: &str, folder: &str) -> anyhow::Result<()> {
    let folder = folder.to_owned();
    update_folders(path, move |list| {
        let Some(pos) = list.iter().position(|entry| entry.path == folder) else {
            return Ok(false);
        };
        list.remove(pos);
        Ok(true)
    })?;
    Ok(())
}

/// Set or clear the alias for a folder entry.
pub fn set_alias(path: &str, folder: &str, alias: &str) -> anyhow::Result<()> {
    let folder = folder.to_owned();
    let alias = alias.to_owned();
    update_folders(path, move |list| {
        let Some(item) = list.iter_mut().find(|entry| entry.path == folder) else {
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

pub struct FoldersPlugin {
    matcher: SkimMatcherV2,
    data: Arc<Mutex<Vec<FolderEntry>>>,
    #[allow(dead_code)]
    watcher: Option<RecommendedWatcher>,
}

impl FoldersPlugin {
    /// Create a new folders plugin.
    pub fn new() -> Self {
        let data = Arc::new(Mutex::new(load_folders(FOLDERS_FILE).unwrap_or_else(
            |error| {
                tracing::error!(%error, "folder startup retained invalid persisted file");
                Vec::new()
            },
        )));
        let data_clone = data.clone();
        let path = FOLDERS_FILE.to_string();
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
                        if let Err(error) = reload_folder_snapshot(&path, &data_clone) {
                            tracing::error!(%error, "invalid folder reload retained last-good state");
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

    fn list_entries(&self, filter: &str) -> Vec<Action> {
        let guard = match self.data.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard
            .iter()
            .filter(|f| {
                self.matcher.fuzzy_match(&f.label, filter).is_some()
                    || self.matcher.fuzzy_match(&f.path, filter).is_some()
                    || f.alias
                        .as_ref()
                        .map(|a| self.matcher.fuzzy_match(a, filter).is_some())
                        .unwrap_or(false)
            })
            .map(|f| {
                let label = f.alias.clone().unwrap_or_else(|| f.label.clone());
                Action {
                    label,
                    desc: f.path.clone(),
                    action: f.path.clone(),
                    args: None,
                }
            })
            .collect()
    }
}

fn reload_folder_snapshot(path: &str, data: &Arc<Mutex<Vec<FolderEntry>>>) -> anyhow::Result<()> {
    let folders = load_folders(path)?;
    if let Ok(mut current) = data.lock() {
        *current = folders;
    }
    Ok(())
}

impl Default for FoldersPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for FoldersPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const ADD_PREFIX: &str = "f add ";
        if let Some(rest) = crate::common::strip_prefix_ci(query, ADD_PREFIX) {
            let path = rest.trim();
            if !path.is_empty() {
                return vec![Action {
                    label: format!("Add folder {path}"),
                    desc: "Folder".into(),
                    action: format!("folder:add:{path}"),
                    args: None,
                }];
            }
        }

        const RM_PREFIX: &str = "f rm ";
        if let Some(rest) = crate::common::strip_prefix_ci(query, RM_PREFIX) {
            let filter = rest.trim();
            let guard = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return Vec::new(),
            };
            return guard
                .iter()
                .filter(|f| {
                    self.matcher.fuzzy_match(&f.label, filter).is_some()
                        || self.matcher.fuzzy_match(&f.path, filter).is_some()
                        || f.alias
                            .as_ref()
                            .map(|a| self.matcher.fuzzy_match(a, filter).is_some())
                            .unwrap_or(false)
                })
                .map(|f| Action {
                    label: format!("Remove folder {} ({})", f.label.clone(), f.path.clone()),
                    desc: f.path.clone(),
                    action: format!("folder:remove:{}", f.path.clone()),
                    args: None,
                })
                .collect();
        }

        const LIST_PREFIX: &str = "f list";
        if let Some(rest) = crate::common::strip_prefix_ci(query, LIST_PREFIX) {
            return self.list_entries(rest.trim());
        }

        const PREFIX: &str = "f";
        let rest = match crate::common::strip_prefix_ci(query, PREFIX) {
            Some(r) => r,
            None => return Vec::new(),
        };
        let filter = rest.trim();
        self.list_entries(filter)
    }

    fn name(&self) -> &str {
        "folders"
    }

    fn description(&self) -> &str {
        "Search and manage favourite folders (prefix: `f`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search", "show_full_path"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "f".into(),
                desc: "Folder".into(),
                action: "query:f ".into(),
                args: None,
            },
            Action {
                label: "f list".into(),
                desc: "Folder".into(),
                action: "query:f list ".into(),
                args: None,
            },
            Action {
                label: "f add".into(),
                desc: "Folder".into(),
                action: "query:f add ".into(),
                args: None,
            },
            Action {
                label: "f rm".into(),
                desc: "Folder".into(),
                action: "query:f rm ".into(),
                args: None,
            },
        ]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["f"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn folder(label: &str, path: &str, alias: Option<&str>) -> FolderEntry {
        FolderEntry {
            label: label.into(),
            path: path.into(),
            alias: alias.map(str::to_owned),
        }
    }

    #[test]
    fn typed_load_preserves_missing_empty_valid_malformed_and_unreadable() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert_eq!(load_folders_typed(&missing).unwrap(), LoadState::Missing);
        assert_eq!(
            load_folders(missing.to_str().unwrap()).unwrap(),
            default_folders()
        );

        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n\t").unwrap();
        assert_eq!(load_folders_typed(&empty).unwrap(), LoadState::Empty);
        assert_eq!(
            load_folders(empty.to_str().unwrap()).unwrap(),
            default_folders()
        );

        let valid = directory.path().join("valid.json");
        let expected = vec![folder("One", "C:\\one", Some("first"))];
        std::fs::write(&valid, serde_json::to_vec(&expected).unwrap()).unwrap();
        assert_eq!(
            load_folders_typed(&valid).unwrap(),
            LoadState::Loaded(expected.clone())
        );
        let saved = directory.path().join("saved.json");
        save_folders(saved.to_str().unwrap(), &expected).unwrap();
        assert_eq!(
            std::fs::read_to_string(saved).unwrap(),
            serde_json::to_string_pretty(&expected).unwrap()
        );

        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "{broken").unwrap();
        assert!(matches!(
            load_folders_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
        assert!(matches!(
            load_folders_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
    }

    #[test]
    fn malformed_file_rejects_add_remove_and_alias_without_changing_bytes() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("folders.json");
        let candidate = directory.path().join("candidate");
        std::fs::create_dir(&candidate).unwrap();
        let invalid = b"not folder JSON";
        std::fs::write(&path, invalid).unwrap();
        let path = path.to_str().unwrap();
        let candidate = candidate.to_str().unwrap();

        assert!(append_folder(path, candidate).is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        assert!(remove_folder(path, candidate).is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        assert!(set_alias(path, candidate, "alias").is_err());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }

    #[test]
    fn concurrent_adds_both_survive() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let first_folder = directory.path().join("first");
        let second_folder = directory.path().join("second");
        std::fs::create_dir(&first_folder).unwrap();
        std::fs::create_dir(&second_folder).unwrap();
        let path = Arc::new(
            directory
                .path()
                .join("folders.json")
                .to_string_lossy()
                .into_owned(),
        );
        let barrier = Arc::new(Barrier::new(3));
        let first = {
            let path = Arc::clone(&path);
            let folder = first_folder.to_string_lossy().into_owned();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                append_folder(&path, &folder).unwrap();
            })
        };
        let second = {
            let path = Arc::clone(&path);
            let folder = second_folder.to_string_lossy().into_owned();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                append_folder(&path, &folder).unwrap();
            })
        };

        barrier.wait();
        first.join().unwrap();
        second.join().unwrap();
        let committed = load_folders(&path).unwrap();
        assert!(
            committed
                .iter()
                .any(|entry| entry.path == first_folder.to_string_lossy())
        );
        assert!(
            committed
                .iter()
                .any(|entry| entry.path == second_folder.to_string_lossy())
        );
    }

    #[test]
    fn invalid_reload_retains_last_good_then_valid_reload_recovers() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("folders.json");
        let initial = vec![folder("Initial", "C:\\initial", None)];
        let data = Arc::new(Mutex::new(initial.clone()));
        std::fs::write(&path, "invalid").unwrap();

        assert!(reload_folder_snapshot(path.to_str().unwrap(), &data).is_err());
        assert_eq!(*data.lock().unwrap(), initial);

        let recovered = vec![folder("Recovered", "C:\\recovered", Some("Recovered"))];
        std::fs::write(&path, serde_json::to_vec_pretty(&recovered).unwrap()).unwrap();
        reload_folder_snapshot(path.to_str().unwrap(), &data).unwrap();
        assert_eq!(*data.lock().unwrap(), recovered);
    }
}
