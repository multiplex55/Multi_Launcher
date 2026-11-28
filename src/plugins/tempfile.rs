use crate::actions::Action;
use crate::plugin::Plugin;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) fn validate_alias(alias: &str) -> anyhow::Result<&str> {
    if alias.contains('\0') {
        anyhow::bail!("alias contains invalid characters");
    }
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
    if let Ok(dir) = std::env::var("ML_TMP_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(test)]
    {
        use std::thread;
        std::env::temp_dir().join(format!("multi_launcher_tmp_{:?}", thread::current().id()))
    }
    #[cfg(not(test))]
    {
        let base = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(std::env::temp_dir);
        base.join("multi_launcher_tmp")
    }
}

fn ensure_dir() -> std::io::Result<PathBuf> {
    let dir = storage_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Create a new unnamed temporary file and return its path.
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

/// Create a new temp file with a specific `alias` and initial `contents`.
/// The filename is prefixed with `temp_` and suffixed with a number if needed.
pub fn create_named_file(alias: &str, contents: &str) -> anyhow::Result<PathBuf> {
    create_named_file_checked(alias, contents, || true).map(|created| created.path)
}

/// A published file and the handle that identifies the exact created object.
pub struct CreatedTempFile {
    pub path: PathBuf,
    pub(crate) file: File,
    #[cfg(windows)]
    pub(crate) resolved_path: PathBuf,
}

/// Stage content privately, then publish the first available name atomically.
pub fn create_named_file_checked(
    alias: &str,
    contents: &str,
    current: impl Fn() -> bool,
) -> anyhow::Result<CreatedTempFile> {
    validate_alias(alias)?;
    anyhow::ensure!(current(), "Temp file request was superseded");
    let dir = ensure_dir()?;
    create_named_file_in(&dir, alias, contents, &current)
}

fn create_named_file_in(
    dir: &Path,
    alias: &str,
    contents: &str,
    current: &dyn Fn() -> bool,
) -> anyhow::Result<CreatedTempFile> {
    validate_alias(alias)?;
    anyhow::ensure!(current(), "Temp file request was superseded");
    let mut staging_builder = tempfile::Builder::new();
    staging_builder.prefix(".multi-launcher-stage-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staging_builder.permissions(fs::Permissions::from_mode(0o700));
    }
    // Resolve once before staging so junction changes cannot redirect publication.
    #[cfg(windows)]
    let native_dir = fs::canonicalize(dir)?;
    #[cfg(windows)]
    let staging = staging_builder.tempdir_in(&native_dir)?;
    #[cfg(not(windows))]
    let staging = staging_builder.tempdir_in(dir)?;
    #[cfg(windows)]
    let mut file = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new().read(true).write(true).create_new(true)
            .access_mode(0x80000000 | 0x40000000 | 0x00010000)
            .share_mode(0x1)
            .open(staging.path().join("contents"))?
    };
    #[cfg(not(windows))]
    let mut file = {
        let mut builder = tempfile::Builder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(fs::Permissions::from_mode(0o666));
        }
        builder.tempfile_in(staging.path())?
    };
    file.write_all(contents.as_bytes())?;
    file.flush()?;
    #[cfg(windows)]
    file.sync_all()?;
    #[cfg(not(windows))]
    file.as_file().sync_all()?;
    let mut idx = 0usize;
    loop {
        let name = if idx == 0 {
            format!("temp_{alias}.txt")
        } else {
            format!("temp_{alias}_{idx}.txt")
        };
        let path = dir.join(&name);
        anyhow::ensure!(current(), "Temp file request was superseded");
        #[cfg(windows)]
        {
            match crate::file_operations::rename_open_file_no_replace_to_path(&file, &native_dir.join(&name)) {
                Ok(()) => return Ok(CreatedTempFile { resolved_path: native_dir.join(&name), path, file }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists
                    || fs::symlink_metadata(native_dir.join(&name)).is_ok() => {},
                Err(error) => return Err(error.into()),
            }
        }
        #[cfg(not(windows))]
        match file.persist_noclobber(&path) {
            Ok(file) => return Ok(CreatedTempFile { path, file }),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists
                || fs::symlink_metadata(&path).is_ok() => {
                file = error.file;
            }
            Err(error) => return Err(error.error.into()),
        }
        idx = idx.checked_add(1).ok_or_else(|| anyhow::anyhow!("No available temp file name"))?;
    }
}

/// Remove a specific file inside the storage directory.
///
/// Does nothing if the file does not exist.
pub fn remove_file(path: &Path) -> anyhow::Result<()> {
    if path.exists() && path.is_file() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Rename a temp file to use the provided `alias`.
/// The resulting file name will always start with `temp_`.
pub fn set_alias(path: &Path, alias: &str) -> anyhow::Result<PathBuf> {
    validate_alias(alias)?;
    let dir = ensure_dir()?;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("txt");
    let new_name = format!("temp_{}.{}", alias, ext);
    let new_path = dir.join(new_name);
    if new_path.exists() && new_path != path {
        anyhow::bail!("file already exists");
    }
    if path != new_path {
        fs::rename(path, &new_path)?;
    }
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

/// Stock list filter, shared with frontends that must scan off the UI thread.
pub fn list_filter(query: &str) -> Option<String> {
    crate::common::strip_prefix_ci(query.trim(), "tmp list")
        .map(|rest| rest.trim().to_lowercase())
}

/// Construct the exact stock list action from its raw path.
pub fn list_action(path: &Path) -> Action {
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    Action { label: name, desc: "Tempfile".into(), action: path.to_string_lossy().into(), args: None }
}

pub fn remove_filter(query: &str) -> Option<String> {
    crate::common::strip_prefix_ci(query.trim(), "tmp rm").map(|rest| rest.trim().to_lowercase())
}
pub fn alias_parts(query: &str) -> Option<(&str, &str)> {
    let rest = crate::common::strip_prefix_ci(query.trim(), "tmp alias")?;
    let mut parts = rest.trim().splitn(2, ' ');
    let file = parts.next()?.trim();
    let alias = parts.next()?.trim();
    (!file.is_empty() && !alias.is_empty()).then_some((file, alias))
}
pub fn remove_action(path: &Path) -> Action {
    let mut action = list_action(path);
    action.label = format!("Remove {}", action.label);
    action.action = format!("tempfile:remove:{}", path.to_string_lossy());
    action
}
pub fn alias_action(path: &Path, name: &str, alias: &str) -> Action {
    Action { label: format!("Set alias {name} -> {alias}"), desc: "Tempfile".into(), action: format!("tempfile:alias:{}|{alias}", path.to_string_lossy()), args: None }
}

impl Plugin for TempfilePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "tmp")
            && rest.is_empty()
        {
            return vec![Action {
                label: "tmp: create".into(),
                desc: "Tempfile".into(),
                action: "tempfile:dialog".into(),
                args: None,
            }];
        }
        const NEW_PREFIX: &str = "tmp new ";
        const CREATE_PREFIX: &str = "tmp create ";
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, NEW_PREFIX)
            .or_else(|| crate::common::strip_prefix_ci(trimmed, CREATE_PREFIX))
        {
            let alias = rest.trim();
            if !alias.is_empty() && validate_alias(alias).is_ok() {
                return vec![Action {
                    label: format!("Create temp file {alias}"),
                    desc: "Tempfile".into(),
                    action: format!("tempfile:new:{alias}"),
                    args: None,
                }];
            }
        } else if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "tmp new")
            .or_else(|| crate::common::strip_prefix_ci(trimmed, "tmp create"))
            && rest.is_empty()
        {
            return vec![Action {
                label: "Create temp file".into(),
                desc: "Tempfile".into(),
                action: "tempfile:new".into(),
                args: None,
            }];
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "tmp open")
            && rest.is_empty()
        {
            return vec![Action {
                label: "Open temp directory".into(),
                desc: "Tempfile".into(),
                action: "tempfile:open".into(),
                args: None,
            }];
        }
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "tmp clear")
            && rest.is_empty()
        {
            return vec![Action {
                label: "Clear temp files".into(),
                desc: "Tempfile".into(),
                action: "tempfile:clear".into(),
                args: None,
            }];
        }
        if let Some(filter) = remove_filter(trimmed) {
            let files = list_files().unwrap_or_default();
            return files
                .into_iter()
                .filter(|p| {
                    filter.is_empty()
                        || p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.to_lowercase().contains(&filter))
                            .unwrap_or(false)
                })
                .map(|p| remove_action(&p))
                .collect();
        }
        if let Some((file, alias)) = alias_parts(trimmed) {
            let files = list_files().unwrap_or_default();
            for p in files {
                if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n == file) {
                    return vec![alias_action(&p, file, alias)];
                }
            }
        }
        if let Some(filter) = list_filter(trimmed) {
            let files = list_files().unwrap_or_default();
            return files
                .into_iter()
                .filter(|p| {
                    filter.is_empty()
                        || p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.to_lowercase().contains(&filter))
                            .unwrap_or(false)
                })
                .map(|p| list_action(&p))
                .collect();
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        <Self as crate::plugin::BuiltinIdentity>::ID
    }

    fn description(&self) -> &str {
        "Manage temporary files (prefix: `tmp`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "tmp".into(),
                desc: "Tempfile".into(),
                action: "query:tmp".into(),
                args: None,
            },
            Action {
                label: "tmp new".into(),
                desc: "Tempfile".into(),
                action: "query:tmp new ".into(),
                args: None,
            },
            Action {
                label: "tmp create".into(),
                desc: "Tempfile".into(),
                action: "query:tmp create ".into(),
                args: None,
            },
            Action {
                label: "tmp open".into(),
                desc: "Tempfile".into(),
                action: "query:tmp open".into(),
                args: None,
            },
            Action {
                label: "tmp clear".into(),
                desc: "Tempfile".into(),
                action: "query:tmp clear".into(),
                args: None,
            },
            Action {
                label: "tmp list".into(),
                desc: "Tempfile".into(),
                action: "query:tmp list".into(),
                args: None,
            },
            Action {
                label: "tmp rm".into(),
                desc: "Tempfile".into(),
                action: "query:tmp rm ".into(),
                args: None,
            },
        ]
    }
}

#[cfg(test)]
mod checked_creation_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn checked_creation_preserves_collision_and_exact_unicode() {
        let root = tempfile::tempdir().unwrap();
        let collision = root.path().join("temp_β.txt");
        let calls = Cell::new(0);
        let created = create_named_file_in(root.path(), "β", "first\nλ", &|| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 { fs::write(&collision, b"foreign").unwrap(); }
            true
        }).unwrap();
        assert_eq!(created.path, root.path().join("temp_β_1.txt"));
        assert_eq!(fs::read(&collision).unwrap(), b"foreign");
        assert_eq!(fs::read(&created.path).unwrap(), "first\nλ".as_bytes());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn published_mode_matches_ordinary_creation_under_umask() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let ordinary = root.path().join("ordinary");
        File::create(&ordinary).unwrap();
        let created = create_named_file_in(root.path(), "mode", "", &|| true).unwrap();
        assert_eq!(fs::metadata(ordinary).unwrap().permissions().mode() & 0o777,
            fs::metadata(created.path).unwrap().permissions().mode() & 0o777);
    }

    #[test]
    fn checked_creation_refuses_revocation_before_publication() {
        let root = tempfile::tempdir().unwrap();
        let calls = Cell::new(0);
        let result = create_named_file_in(root.path(), "safe", "value", &|| {
            calls.set(calls.get() + 1);
            calls.get() == 1
        });
        assert!(result.is_err());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        assert!(create_named_file_in(root.path(), "bad/alias", "value", &|| true).is_err());
        assert!(create_named_file_in(root.path(), "bad\0alias", "value", &|| true).is_err());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
