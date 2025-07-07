use crate::actions::Action;
use crate::plugin::Plugin;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};

pub const FOLDERS_FILE: &str = "folders.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct FolderEntry {
    pub label: String,
    pub path: String,
    #[serde(default)]
    pub alias: Option<String>,
}

pub fn default_folders() -> Vec<FolderEntry> {
    let mut out = Vec::new();
    if let Some(p) = dirs_next::home_dir() {
        out.push(FolderEntry { label: "Home".into(), path: p.to_string_lossy().into(), alias: None });
    }
    if let Some(p) = dirs_next::download_dir() {
        out.push(FolderEntry { label: "Downloads".into(), path: p.to_string_lossy().into(), alias: None });
    }
    if let Some(p) = dirs_next::desktop_dir() {
        out.push(FolderEntry { label: "Desktop".into(), path: p.to_string_lossy().into(), alias: None });
    }
    if let Some(p) = dirs_next::document_dir() {
        out.push(FolderEntry { label: "Documents".into(), path: p.to_string_lossy().into(), alias: None });
    }
    out
}

pub fn load_folders(path: &str) -> anyhow::Result<Vec<FolderEntry>> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.is_empty() {
        return Ok(default_folders());
    }
    let list: Vec<FolderEntry> = serde_json::from_str(&content)?;
    Ok(list)
}

pub fn save_folders(path: &str, folders: &[FolderEntry]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(folders)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn append_folder(path: &str, folder: &str) -> anyhow::Result<()> {
    let mut list = load_folders(path).unwrap_or_else(|_| default_folders());
    if !list.iter().any(|f| f.path == folder) {
        let label = std::path::Path::new(folder)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| folder.to_string());
        list.push(FolderEntry { label, path: folder.to_string(), alias: None });
        save_folders(path, &list)?;
    }
    Ok(())
}

pub fn remove_folder(path: &str, folder: &str) -> anyhow::Result<()> {
    let mut list = load_folders(path).unwrap_or_else(|_| default_folders());
    if let Some(pos) = list.iter().position(|f| f.path == folder) {
        list.remove(pos);
        save_folders(path, &list)?;
    }
    Ok(())
}

pub fn set_alias(path: &str, folder: &str, alias: &str) -> anyhow::Result<()> {
    let mut list = load_folders(path).unwrap_or_else(|_| default_folders());
    if let Some(item) = list.iter_mut().find(|f| f.path == folder) {
        item.alias = if alias.is_empty() { None } else { Some(alias.to_string()) };
        save_folders(path, &list)?;
    }
    Ok(())
}

pub struct FoldersPlugin {
    matcher: SkimMatcherV2,
}

impl FoldersPlugin {
    pub fn new() -> Self {
        Self { matcher: SkimMatcherV2::default() }
    }
}

impl Default for FoldersPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for FoldersPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if let Some(path) = query.strip_prefix("f add ") {
            let path = path.trim();
            if !path.is_empty() {
                return vec![Action {
                    label: format!("Add folder {path}"),
                    desc: "Folder".into(),
                    action: format!("folder:add:{path}"),
                    args: None,
                }];
            }
        }

        if let Some(pattern) = query.strip_prefix("f rm ") {
            let filter = pattern.trim();
            let folders = load_folders(FOLDERS_FILE).unwrap_or_else(|_| default_folders());
            return folders
                .into_iter()
                .filter(|f| {
                    self.matcher.fuzzy_match(&f.label, filter).is_some()
                        || self.matcher.fuzzy_match(&f.path, filter).is_some()
                        || f
                            .alias
                            .as_ref()
                            .map(|a| self.matcher.fuzzy_match(a, filter).is_some())
                            .unwrap_or(false)
                })
                .map(|f| Action {
                    label: format!("Remove folder {} ({})", f.label, f.path),
                    desc: f.path.clone(),
                    action: format!("folder:remove:{}", f.path),
                    args: None,
                })
                .collect();
        }

        if !query.starts_with("f") {
            return Vec::new();
        }
        let filter = query.strip_prefix("f").unwrap_or("").trim();
        let folders = load_folders(FOLDERS_FILE).unwrap_or_else(|_| default_folders());
        folders
            .into_iter()
            .filter(|f| {
                self.matcher.fuzzy_match(&f.label, filter).is_some()
                    || self.matcher.fuzzy_match(&f.path, filter).is_some()
                    || f
                        .alias
                        .as_ref()
                        .map(|a| self.matcher.fuzzy_match(a, filter).is_some())
                        .unwrap_or(false)
            })
            .map(|f| {
                let label = f.alias.clone().unwrap_or_else(|| f.label.clone());
                Action {
                    label,
                    desc: f.path.clone(),
                    action: f.path,
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
}
