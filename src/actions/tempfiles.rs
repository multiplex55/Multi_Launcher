use anyhow::Context;
use std::path::{Path, PathBuf};

/// Publication succeeds independently from the operating-system association.
pub enum CreateAndOpenOutcome {
    Opened(PathBuf),
    Published { path: PathBuf, reason: PublishedReason },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishedReason {
    Superseded,
    Changed,
    OpenFailed,
}

/// Create one complete file, then open only that exact published object.
pub fn create_and_open_checked(
    alias: &str,
    contents: &str,
    current: impl Fn() -> bool,
) -> anyhow::Result<CreateAndOpenOutcome> {
    let created = crate::plugins::tempfile::create_named_file_checked(alias, contents, &current)?;
    Ok(open_created_with(created, &current, |path, guard| {
        crate::actions::exec::launch_association_if_current(path.as_os_str(), guard)
    }))
}

fn open_created_with(
    created: crate::plugins::tempfile::CreatedTempFile,
    current: &dyn Fn() -> bool,
    launch: impl FnOnce(&Path, &dyn Fn() -> bool) -> anyhow::Result<()>,
) -> CreateAndOpenOutcome {
    let path = created.path;
    #[cfg(windows)]
    let expected_identity = crate::file_operations::native_file_identity(&created.file).ok();
    #[cfg(windows)]
    let launch_path = created.resolved_path;
    #[cfg(windows)]
    drop(created.file);
    #[cfg(windows)]
    let held = open_published_guard(&launch_path, expected_identity);
    #[cfg(windows)]
    let same_file = || held.is_some();
    #[cfg(not(windows))]
    let same_file = || same_published_file(&created.file, &path);
    if !current() { return CreateAndOpenOutcome::Published { path, reason: PublishedReason::Superseded }; }
    if !same_file() { return CreateAndOpenOutcome::Published { path, reason: PublishedReason::Changed }; }
    let reason = std::cell::Cell::new(None);
    #[cfg(not(windows))]
    let launch_path = path.clone();
    #[cfg(windows)]
    let shell_path = shell_path_from_resolved(&launch_path);
    #[cfg(not(windows))]
    let shell_path = launch_path.clone();
    let result = launch(&shell_path, &|| {
        if !current() { reason.set(Some(PublishedReason::Superseded)); return false; }
        if !same_file() { reason.set(Some(PublishedReason::Changed)); return false; }
        #[cfg(windows)]
        if open_published_guard(&launch_path, expected_identity).is_none()
            || open_published_guard(&shell_path, expected_identity).is_none() {
            reason.set(Some(PublishedReason::Changed)); return false;
        }
        if !current() { reason.set(Some(PublishedReason::Superseded)); return false; }
        true
    });
    match result {
        Ok(()) => CreateAndOpenOutcome::Opened(path),
        Err(_) => CreateAndOpenOutcome::Published { path, reason: reason.get().unwrap_or(PublishedReason::OpenFailed) },
    }
}

#[cfg(windows)]
fn shell_path_from_resolved(path: &Path) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    let unc: Vec<u16> = r"\\?\UNC\".encode_utf16().collect();
    let verbatim: Vec<u16> = r"\\?\".encode_utf16().collect();
    let output = if units.starts_with(&unc) {
        let mut result = vec![b'\\' as u16, b'\\' as u16];
        result.extend_from_slice(&units[unc.len()..]);
        result
    } else if units.starts_with(&verbatim) && units.get(verbatim.len() + 1) == Some(&(b':' as u16)) {
        units[verbatim.len()..].to_vec()
    } else { units };
    PathBuf::from(std::ffi::OsString::from_wide(&output))
}

#[cfg(windows)]
fn open_published_guard(
    path: &Path,
    expected: Option<crate::file_operations::NativeFileIdentity>,
) -> Option<std::fs::File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let expected = expected?;
    let file = std::fs::OpenOptions::new().read(true)
        .share_mode(0x1 | 0x2)
        .custom_flags(0x00200000)
        .open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0
        || crate::file_operations::native_file_identity(&file).ok()? != expected {
        return None;
    }
    Some(file)
}

#[cfg(not(windows))]
fn same_published_file(original: &std::fs::File, path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else { return false; };
    if !metadata.file_type().is_file() { return false; }
    let Ok(current) = std::fs::File::open(path) else { return false; };
    same_file_identity(original, &current)
}

#[cfg(windows)]
fn same_file_identity(original: &std::fs::File, current: &std::fs::File) -> bool {
    match (crate::file_operations::native_file_identity(original), crate::file_operations::native_file_identity(current)) {
        (Ok(first), Ok(second)) => first == second,
        _ => false,
    }
}

#[cfg(unix)]
fn same_file_identity(original: &std::fs::File, current: &std::fs::File) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (original.metadata(), current.metadata()) {
        (Ok(first), Ok(second)) => first.dev() == second.dev() && first.ino() == second.ino(),
        _ => false,
    }
}

#[cfg(not(any(windows, unix)))]
fn same_file_identity(_: &std::fs::File, _: &std::fs::File) -> bool { false }

pub fn new(alias: Option<&str>) -> anyhow::Result<()> {
    let path = if let Some(a) = alias {
        crate::plugins::tempfile::create_named_file(a, "")?
    } else {
        crate::plugins::tempfile::create_file()?
    };
    open::that(&path)?;
    Ok(())
}

pub fn open_dir() -> anyhow::Result<()> {
    let dir = crate::plugins::tempfile::storage_dir();
    std::fs::create_dir_all(&dir)?;
    open::that(dir)?;
    Ok(())
}

pub fn open_file(path: &str) -> anyhow::Result<()> {
    open::that(Path::new(path))?;
    Ok(())
}

pub fn clear() -> anyhow::Result<()> {
    crate::plugins::tempfile::clear_files()?;
    Ok(())
}

pub fn remove(path: &str) -> anyhow::Result<()> {
    crate::plugins::tempfile::remove_file(Path::new(path))?;
    Ok(())
}

pub fn set_alias(path: &str, alias: &str) -> anyhow::Result<()> {
    crate::plugins::tempfile::set_alias(Path::new(path), alias)
        .context("failed to rename tempfile")?;
    Ok(())
}

#[cfg(test)]
mod checked_creation_tests {
    use super::*;
    use std::io::Write;

    fn owned_file(root: &Path) -> crate::plugins::tempfile::CreatedTempFile {
        let path = root.join("owned.txt");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"owned").unwrap();
        file.flush().unwrap();
        crate::plugins::tempfile::CreatedTempFile {
            #[cfg(windows)]
            resolved_path: path.clone(),
            path, file
        }
    }

    #[cfg(windows)]
    #[test]
    fn shell_projection_keeps_drive_and_unc_targets() {
        assert_eq!(shell_path_from_resolved(Path::new(r"\\?\C:\tmp\é.txt")), PathBuf::from(r"C:\tmp\é.txt"));
        assert_eq!(shell_path_from_resolved(Path::new(r"\\?\UNC\server\share\é.txt")), PathBuf::from(r"\\server\share\é.txt"));
    }

    #[test]
    fn published_file_survives_failed_or_revoked_open() {
        let root = tempfile::tempdir().unwrap();
        let created = owned_file(root.path());
        let outcome = open_created_with(created, &|| true, |_, guard| {
            assert!(guard());
            anyhow::bail!("injected association failure")
        });
        let CreateAndOpenOutcome::Published { path, reason } = outcome else { panic!("expected retained file") };
        assert_eq!(reason, PublishedReason::OpenFailed);
        assert_eq!(std::fs::read(path).unwrap(), b"owned");

        let created = owned_file(root.path());
        let outcome = open_created_with(created, &|| false, |_, _| panic!("revoked request launched"));
        assert!(matches!(outcome, CreateAndOpenOutcome::Published { reason: PublishedReason::Superseded, .. }));
    }

    #[test]
    fn association_guard_refuses_a_replaced_published_file() {
        let root = tempfile::tempdir().unwrap();
        let created = owned_file(root.path());
        let outcome = open_created_with(created, &|| true, |path, guard| {
            #[cfg(windows)]
            {
                assert!(std::fs::rename(path, root.path().join("moved.txt")).is_err());
                assert!(guard());
                anyhow::bail!("guard retained the object")
            }
            #[cfg(not(windows))]
            std::fs::rename(path, root.path().join("moved.txt")).unwrap();
            #[cfg(not(windows))]
            std::fs::write(path, b"replacement").unwrap();
            #[cfg(not(windows))]
            {
                assert!(!guard());
                anyhow::bail!("guard rejected replacement")
            }
        });
        #[cfg(not(windows))]
        {
            assert!(matches!(outcome, CreateAndOpenOutcome::Published { reason: PublishedReason::Changed, .. }));
            assert_eq!(std::fs::read(root.path().join("owned.txt")).unwrap(), b"replacement");
            assert_eq!(std::fs::read(root.path().join("moved.txt")).unwrap(), b"owned");
        }
        #[cfg(windows)]
        assert!(matches!(outcome, CreateAndOpenOutcome::Published { reason: PublishedReason::OpenFailed, .. }));
    }
}
