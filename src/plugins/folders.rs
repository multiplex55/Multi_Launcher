use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const FOLDERS_FILE: &str = "folders.json";

#[derive(Serialize, Deserialize, Clone)]
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
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.is_empty() {
        return Ok(default_folders());
    }
    let list: Vec<FolderEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

/// Save `folders` to `path` in JSON format.
pub fn save_folders(path: &str, folders: &[FolderEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(folders)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Append a folder path to the list stored at `path`.
///
/// Returns an error if the folder does not exist.
pub fn append_folder(path: &str, folder: &str) -> anyhow::Result<()> {
    if !std::path::Path::new(folder).exists() {
        anyhow::bail!("folder does not exist: {folder}");
    }

    let mut list = load_folders(path).unwrap_or_else(|_| default_folders());
    if !list.iter().any(|f| f.path == folder) {
        let label = std::path::Path::new(folder)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| folder.to_string());
        list.push(FolderEntry {
            label,
            path: folder.to_string(),
            alias: None,
        });
        save_folders(path, &list)?;
    }
    Ok(())
}

/// Remove a folder entry matching `folder` from the file at `path`.
pub fn remove_folder(path: &str, folder: &str) -> anyhow::Result<()> {
    let mut list = load_folders(path).unwrap_or_else(|_| default_folders());
    if let Some(pos) = list.iter().position(|f| f.path == folder) {
        list.remove(pos);
        save_folders(path, &list)?;
    }
    Ok(())
}

/// Set or clear the alias for a folder entry.
pub fn set_alias(path: &str, folder: &str, alias: &str) -> anyhow::Result<()> {
    let mut list = load_folders(path).unwrap_or_else(|_| default_folders());
    if let Some(item) = list.iter_mut().find(|f| f.path == folder) {
        item.alias = if alias.is_empty() {
            None
        } else {
            Some(alias.to_string())
        };
        save_folders(path, &list)?;
    }
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
        let data = Arc::new(Mutex::new(
            load_folders(FOLDERS_FILE).unwrap_or_else(|_| default_folders()),
        ));
        let data_clone = data.clone();
        let path = FOLDERS_FILE.to_string();
        let mut watcher = RecommendedWatcher::new(
            {
                let path = path.clone();
                move |res: notify::Result<notify::Event>| {
                    if let Ok(event) = res {
                        if matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        ) {
                            let list = load_folders(&path).unwrap_or_else(|_| default_folders());
                            if let Ok(mut lock) = data_clone.lock() {
                                *lock = list;
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

        const PREFIX: &str = "f";
        let rest = match crate::common::strip_prefix_ci(query, PREFIX) {
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
}
