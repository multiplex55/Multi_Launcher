use crate::actions::Action;
use crate::plugin::Plugin;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

/// Return the directory used to store temporary files.
pub fn storage_dir() -> PathBuf {
    std::env::temp_dir().join("multi_launcher_tmp")
}

fn ensure_dir() -> std::io::Result<PathBuf> {
    let dir = storage_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Create a new temporary file and return its path.
pub fn create_file() -> anyhow::Result<PathBuf> {
    let dir = ensure_dir()?;
    let mut idx = 0;
    loop {
        let path = dir.join(format!("temp_{idx}.txt"));
        if !path.exists() {
            File::create(&path)?;
            return Ok(path);
        }
        idx += 1;
    }
}

/// Remove all files inside the storage directory.
pub fn clear_files() -> anyhow::Result<()> {
    let dir = ensure_dir()?;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().is_file() {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// Return all files in the storage directory.
pub fn list_files() -> anyhow::Result<Vec<PathBuf>> {
    let dir = ensure_dir()?;
    let mut list = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().is_file() {
            list.push(entry.path());
        }
    }
    Ok(list)
}

pub struct TempfilePlugin;

impl Plugin for TempfilePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed == "tmp new" {
            return vec![Action {
                label: "Create temp file".into(),
                desc: "Tempfile".into(),
                action: "tempfile:new".into(),
                args: None,
            }];
        }
        if trimmed == "tmp open" {
            return vec![Action {
                label: "Open temp directory".into(),
                desc: "Tempfile".into(),
                action: "tempfile:open".into(),
                args: None,
            }];
        }
        if trimmed == "tmp clear" {
            return vec![Action {
                label: "Clear temp files".into(),
                desc: "Tempfile".into(),
                action: "tempfile:clear".into(),
                args: None,
            }];
        }
        if let Some(filter) = trimmed.strip_prefix("tmp list") {
            let filter = filter.trim();
            let files = list_files().unwrap_or_default();
            return files
                .into_iter()
                .filter(|p| {
                    filter.is_empty()
                        || p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.contains(filter))
                            .unwrap_or(false)
                })
                .map(|p| Action {
                    label: p.file_name().unwrap().to_string_lossy().into(),
                    desc: "Tempfile".into(),
                    action: p.to_string_lossy().into(),
                    args: None,
                })
                .collect();
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "tempfile"
    }

    fn description(&self) -> &str {
        "Manage temporary files (prefix: `tmp`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}
