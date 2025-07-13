use crate::actions::Action;
use crate::plugin::Plugin;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

fn validate_alias(alias: &str) -> anyhow::Result<&str> {
    if alias.is_empty() {
        anyhow::bail!("alias cannot be empty");
    }
    #[cfg(windows)]
    let invalid = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
    #[cfg(not(windows))]
    let invalid = ['\\', '/'];
    if alias.chars().any(|c| invalid.contains(&c)) {
        anyhow::bail!("alias contains invalid characters");
    }
    Ok(alias)
}

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

/// Create a new temp file with a specific alias and contents. The filename is
/// prefixed with `temp_` and suffixed with a number if needed.
pub fn create_named_file(alias: &str, contents: &str) -> anyhow::Result<PathBuf> {
    validate_alias(alias)?;
    let dir = ensure_dir()?;
    let mut idx = 0;
    loop {
        let name = if idx == 0 {
            format!("temp_{alias}.txt")
        } else {
            format!("temp_{alias}_{idx}.txt")
        };
        let path = dir.join(name);
        if !path.exists() {
            fs::write(&path, contents)?;
            return Ok(path);
        }
        idx += 1;
    }
}

/// Remove a specific file inside the storage directory.
pub fn remove_file(path: &Path) -> anyhow::Result<()> {
    if path.exists() && path.is_file() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Rename a temp file to use the provided alias.
/// The resulting file name will always start with `temp_`.
pub fn set_alias(path: &Path, alias: &str) -> anyhow::Result<PathBuf> {
    validate_alias(alias)?;
    let dir = ensure_dir()?;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("txt");
    let new_name = format!("temp_{}.{}", alias, ext);
    let new_path = dir.join(new_name);
    fs::rename(path, &new_path)?;
    Ok(new_path)
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
        if trimmed == "tmp" {
            return vec![Action {
                label: "tmp: create".into(),
                desc: "Tempfile".into(),
                action: "tempfile:dialog".into(),
                args: None,
            }];
        }
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
        if let Some(filter) = trimmed.strip_prefix("tmp rm") {
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
                    label: format!("Remove {}", p.file_name().unwrap().to_string_lossy()),
                    desc: "Tempfile".into(),
                    action: format!("tempfile:remove:{}", p.to_string_lossy()),
                    args: None,
                })
                .collect();
        }
        if let Some(rest) = trimmed.strip_prefix("tmp alias") {
            let mut parts = rest.trim().splitn(2, ' ');
            if let (Some(file), Some(alias)) = (parts.next(), parts.next()) {
                let file = file.trim();
                let alias = alias.trim();
                if !file.is_empty() && !alias.is_empty() {
                    let files = list_files().unwrap_or_default();
                    for p in files {
                        if p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n == file)
                            .unwrap_or(false)
                        {
                            return vec![Action {
                                label: format!("Set alias {} -> {}", file, alias),
                                desc: "Tempfile".into(),
                                action: format!("tempfile:alias:{}|{}", p.to_string_lossy(), alias),
                                args: None,
                            }];
                        }
                    }
                }
            }
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
