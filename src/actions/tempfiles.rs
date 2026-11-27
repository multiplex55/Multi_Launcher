use anyhow::Context;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::Arc;

#[cfg(windows)]
type FileIdentity = (crate::file_operations::NativeFileIdentity, u64);
#[cfg(unix)]
type FileIdentity = (u64, u64);
#[cfg(not(any(windows, unix)))]
type FileIdentity = ();

fn file_identity(file: &std::fs::File) -> Option<FileIdentity> {
    #[cfg(windows)]
    { use std::os::windows::fs::MetadataExt; Some((crate::file_operations::native_file_identity(file).ok()?, file.metadata().ok()?.creation_time())) }
    #[cfg(unix)]
    { use std::os::unix::fs::MetadataExt; let metadata = file.metadata().ok()?; Some((metadata.dev(), metadata.ino())) }
    #[cfg(not(any(windows, unix)))]
    { let _ = file; None }
}

/// A stock list row bound to the exact file observed during the background scan.
#[derive(Clone)]
pub struct ListedTempFile {
    pub action: crate::actions::Action,
    path: PathBuf,
    file_identity: FileIdentity,
    root_path: PathBuf,
    root_identity: FileIdentity,
    root_resolved: PathBuf,
    operation: ListedOperation,
    #[cfg(unix)]
    _root_hold: Arc<std::fs::File>,
    #[cfg(unix)]
    _file_hold: Arc<std::fs::File>,
    #[cfg(windows)]
    resolved_path: PathBuf,
}

#[derive(Clone)]
enum ListedOperation { Open, Remove, Alias(String) }

/// Scan without creating a missing storage directory. The caller owns query
/// generation and can revoke the scan between entries.
pub fn list_checked_in(dir: &Path, query: &str, current: impl Fn() -> bool) -> anyhow::Result<Vec<ListedTempFile>> {
    enum Projection { List(String), Remove(String), Alias(String, String) }
    let projection = if let Some(filter) = crate::plugins::tempfile::remove_filter(query) {
        Projection::Remove(filter)
    } else if let Some((file, alias)) = crate::plugins::tempfile::alias_parts(query) {
        Projection::Alias(file.to_owned(), alias.to_owned())
    } else if let Some(filter) = crate::plugins::tempfile::list_filter(query) {
        Projection::List(filter)
    } else { return Ok(Vec::new()); };
    anyhow::ensure!(current(), "Temp file search was superseded");
    let metadata = match std::fs::symlink_metadata(dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(metadata.file_type().is_dir() && !is_reparse(&metadata), "Temp file storage is not a regular directory");
    #[cfg(unix)]
    let root_file = Arc::new(open_storage_root(dir, true)?);
    #[cfg(not(unix))]
    let root_file = open_storage_root(dir, true)?;
    let root_identity = file_identity(&root_file).ok_or_else(|| anyhow::anyhow!("Could not identify temporary file storage"))?;
    #[cfg(windows)]
    let canonical = crate::file_operations::final_path_for_handle_verbatim(&root_file)?;
    #[cfg(not(windows))]
    let canonical = std::fs::canonicalize(dir)?;
    anyhow::ensure!(root_matches(dir, root_identity, &canonical), "Temp file storage changed during search");
    let scan_guard = open_storage_root(dir, false)?;
    anyhow::ensure!(file_identity(&scan_guard) == Some(root_identity), "Temp file storage changed during search");
    let mut rows = Vec::new();
    for entry in std::fs::read_dir(&canonical)? {
        anyhow::ensure!(current(), "Temp file search was superseded");
        anyhow::ensure!(root_matches(dir, root_identity, &canonical), "Temp file storage changed during search");
        let entry = entry?;
        let name = entry.file_name();
        let path = dir.join(&name);
        let Some(name_text) = name.to_str() else { continue; };
        let matches = match &projection {
            Projection::List(filter) | Projection::Remove(filter) => name_text.to_lowercase().contains(filter),
            Projection::Alias(file, _) => name_text == file,
        };
        if !matches
            || !path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("txt")) { continue; }
        let Ok(metadata) = std::fs::symlink_metadata(&path) else { continue; };
        if !metadata.file_type().is_file() || is_reparse(&metadata) { continue; }
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new().access_mode(0x80).share_mode(0x1 | 0x2 | 0x4).custom_flags(0x00200000).open(&path)
        };
        #[cfg(not(windows))]
        let file = std::fs::File::open(&path);
        let Ok(file) = file else { continue; };
        let Ok(opened) = file.metadata() else { continue; };
        if !opened.is_file() || is_reparse(&opened) { continue; }
        #[cfg(windows)]
        let resolved_path = match crate::file_operations::final_path_for_handle_verbatim(&file) {
            Ok(path) => path,
            Err(_) => continue,
        };
        #[cfg(windows)]
        if resolved_path.parent() != Some(canonical.as_path()) { continue; }
        // A file swapped or redirected during enumeration is not a candidate.
        let Ok(resolved) = std::fs::canonicalize(&path) else { continue; };
        #[cfg(windows)]
        if resolved.parent() != Some(canonical.as_path()) { continue; }
        #[cfg(not(windows))]
        if resolved.parent() != Some(canonical.as_path()) { continue; }
        let Some(identity) = file_identity(&file) else { continue; };
        #[cfg(windows)]
        if open_published_guard(&path, Some(identity.0)).is_none() { continue; }
        #[cfg(not(windows))]
        if !same_published_file(&file, &path) { continue; }
        let (action, operation) = match &projection {
            Projection::List(_) => (crate::plugins::tempfile::list_action(&path), ListedOperation::Open),
            Projection::Remove(_) => (crate::plugins::tempfile::remove_action(&path), ListedOperation::Remove),
            Projection::Alias(file, alias) => (crate::plugins::tempfile::alias_action(&path, file, alias), ListedOperation::Alias(alias.clone())),
        };
        rows.push(ListedTempFile { action, operation, path, file_identity: identity, root_path: dir.to_owned(), root_identity, root_resolved: canonical.clone(), #[cfg(windows)] resolved_path, #[cfg(unix)] _root_hold: root_file.clone(), #[cfg(unix)] _file_hold: Arc::new(file) });
        if matches!(projection, Projection::Alias(..)) { break; }
    }
    anyhow::ensure!(current(), "Temp file search was superseded");
    anyhow::ensure!(root_matches(dir, root_identity, &canonical), "Temp file storage changed during search");
    drop(scan_guard);
    Ok(rows)
}

fn open_storage_root(path: &Path, share_delete: bool) -> std::io::Result<std::fs::File> {
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().access_mode(0x80 | if share_delete { 0 } else { 0x1 }).share_mode(0x1 | 0x2 | if share_delete { 0x4 } else { 0 })
            .custom_flags(0x02000000 | 0x00200000).open(path)?
    };
    #[cfg(not(windows))]
    let file = { let _ = share_delete; std::fs::File::open(path)? };
    let metadata = file.metadata()?;
    if !metadata.is_dir() || is_reparse(&metadata) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Temp file storage is not a regular directory"));
    }
    Ok(file)
}

fn root_matches(path: &Path, expected: FileIdentity, resolved: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else { return false; };
    if !metadata.file_type().is_dir() || is_reparse(&metadata) { return false; }
    let Ok(current) = open_storage_root(path, true) else { return false; };
    if file_identity(&current) != Some(expected) { return false; }
    #[cfg(windows)]
    { crate::file_operations::final_path_for_handle_verbatim(&current).ok().as_deref() == Some(resolved) }
    #[cfg(not(windows))]
    { std::fs::canonicalize(path).ok().as_deref() == Some(resolved) }
}

fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    { use std::os::windows::fs::MetadataExt; metadata.file_attributes() & 0x400 != 0 }
    #[cfg(not(windows))]
    { metadata.file_type().is_symlink() }
}

pub fn open_listed_if_current(row: &ListedTempFile, current: impl Fn() -> bool) -> anyhow::Result<CreateAndOpenOutcome> {
    anyhow::ensure!(matches!(row.operation, ListedOperation::Open), "Not a temporary file Open row");
    open_listed_with_effect(row, current, || true)
}

pub fn open_listed_with_effect(row: &ListedTempFile, current: impl Fn() -> bool, before_effect: impl Fn() -> bool) -> anyhow::Result<CreateAndOpenOutcome> {
    anyhow::ensure!(matches!(row.operation, ListedOperation::Open), "Not a temporary file Open row");
    open_listed_with(row, &current, |path, guard| crate::actions::exec::launch_association_if_current(path.as_os_str(), || guard() && before_effect()))
}

/// Rename or remove exactly the observed object using its opened native handle.
#[cfg(windows)]
pub fn mutate_listed_with_effect(row: &ListedTempFile, current: impl Fn() -> bool, before_effect: impl Fn() -> bool) -> anyhow::Result<()> {
    use std::path::{Component, Prefix};
    let target = match &row.operation {
        ListedOperation::Open => anyhow::bail!("Not a temporary file mutation row"),
        ListedOperation::Remove => None,
        ListedOperation::Alias(alias) => {
            let alias = crate::plugins::tempfile::validate_alias(alias)?;
            let extension = row.path.extension().and_then(|value| value.to_str()).unwrap_or("txt");
            Some(row.root_resolved.join(format!("temp_{alias}.{extension}")))
        }
    };
    let drive = match row.root_resolved.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() { Prefix::VerbatimDisk(drive) | Prefix::Disk(drive) => drive, _ => anyhow::bail!("Temporary file mutation requires a local drive path") },
        _ => anyhow::bail!("Temporary file mutation requires a local drive path"),
    };
    let drive_root = [u16::from(drive), b':' as u16, b'\\' as u16, 0];
    // The stack array is a NUL-terminated drive root and remains live for the call.
    let drive_type = unsafe { windows::Win32::Storage::FileSystem::GetDriveTypeW(windows::core::PCWSTR(drive_root.as_ptr())) };
    anyhow::ensure!(matches!(drive_type, 2 | 3 | 5 | 6), "Temporary file mutation requires an available local drive");
    anyhow::ensure!(current(), "Temporary file request was superseded");
    let root = open_storage_root(&row.root_path, false)?;
    anyhow::ensure!(file_identity(&root) == Some(row.root_identity) && root_matches(&row.root_path, row.root_identity, &row.root_resolved), "Temporary file storage changed");
    let file = crate::file_operations::open_tempfile_mutation_file(&row.resolved_path)?;
    anyhow::ensure!(file_identity(&file) == Some(row.file_identity)
        && crate::file_operations::final_path_for_handle_verbatim(&file).ok().as_ref() == Some(&row.resolved_path)
        && row.resolved_path.parent() == Some(row.root_resolved.as_path()), "Temporary file changed");
    let same_name = target.as_ref().is_some_and(|target| target == &row.resolved_path);
    let target_vacant = || -> anyhow::Result<()> {
        if let Some(target) = target.as_ref().filter(|_| !same_name) {
            match std::fs::symlink_metadata(target) {
                Ok(_) => anyhow::bail!("Temporary file alias target already exists"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    };
    target_vacant()?;
    anyhow::ensure!(root_matches(&row.root_path, row.root_identity, &row.root_resolved) && current() && before_effect(), "Temporary file request was superseded");
    target_vacant()?;
    anyhow::ensure!(root_matches(&row.root_path, row.root_identity, &row.root_resolved) && current(), "Temporary file request was superseded");
    match target {
        Some(_) if same_name => {},
        Some(target) => crate::file_operations::rename_open_file_no_replace_to_path(&file, &target)?,
        None => crate::file_operations::delete_open_file(&file)?,
    }
    drop(file);
    drop(root);
    Ok(())
}

#[cfg(not(windows))]
pub fn mutate_listed_with_effect(_: &ListedTempFile, _: impl Fn() -> bool, _: impl Fn() -> bool) -> anyhow::Result<()> {
    anyhow::bail!("Checked temporary file mutation requires Windows")
}

fn open_listed_with(row: &ListedTempFile, current: &dyn Fn() -> bool, launch: impl FnOnce(&Path, &dyn Fn() -> bool) -> anyhow::Result<()>) -> anyhow::Result<CreateAndOpenOutcome> {
    let held_root = open_storage_root(&row.root_path, false).ok().filter(|held| file_identity(held) == Some(row.root_identity));
    #[cfg(windows)]
    let held_file = open_published_guard(&row.path, Some(row.file_identity.0)).filter(|file| file_identity(file) == Some(row.file_identity));
    #[cfg(not(windows))]
    let held_file = std::fs::File::open(&row.path).ok().filter(|file| file_identity(file) == Some(row.file_identity));
    let same_original = || {
        if held_root.is_none() || held_file.is_none() || !root_matches(&row.root_path, row.root_identity, &row.root_resolved) { return false; }
        let Ok(metadata) = std::fs::symlink_metadata(&row.path) else { return false; };
        if !metadata.file_type().is_file() || is_reparse(&metadata) { return false; }
        #[cfg(windows)]
        {
            let Some(file) = open_published_guard(&row.path, Some(row.file_identity.0)) else { return false; };
            file_identity(&file) == Some(row.file_identity)
                && crate::file_operations::final_path_for_handle_verbatim(&file).ok().as_ref() == Some(&row.resolved_path)
        }
        #[cfg(not(windows))]
        { std::fs::File::open(&row.path).ok().is_some_and(|file| file_identity(&file) == Some(row.file_identity)) }
    };
    if !current() { return Ok(CreateAndOpenOutcome::Published { path: row.path.clone(), reason: PublishedReason::Superseded }); }
    if !same_original() { return Ok(CreateAndOpenOutcome::Published { path: row.path.clone(), reason: PublishedReason::Changed }); }
    #[cfg(windows)]
    let shell_path = shell_path_from_resolved(&row.resolved_path);
    #[cfg(not(windows))]
    let shell_path = row.path.clone();
    let reason = std::cell::Cell::new(None);
    let result = launch(&shell_path, &|| {
        if !current() { reason.set(Some(PublishedReason::Superseded)); return false; }
        if !same_original() { reason.set(Some(PublishedReason::Changed)); return false; }
        #[cfg(windows)]
        if open_published_guard(&shell_path, Some(row.file_identity.0)).is_none_or(|file| file_identity(&file) != Some(row.file_identity)) {
            reason.set(Some(PublishedReason::Changed)); return false;
        }
        if !current() { reason.set(Some(PublishedReason::Superseded)); return false; }
        true
    });
    Ok(match result {
        Ok(()) => CreateAndOpenOutcome::Opened(row.path.clone()),
        Err(_) => CreateAndOpenOutcome::Published { path: row.path.clone(), reason: reason.get().unwrap_or(PublishedReason::OpenFailed) },
    })
}

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

    #[test]
    fn listed_files_match_stock_unicode_without_creating_missing_directory() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing");
        assert!(list_checked_in(&missing, "tmp list", || true).unwrap().is_empty());
        assert!(!missing.exists());
        std::fs::write(root.path().join("Café.txt"), b"one").unwrap();
        std::fs::write(root.path().join("other.md"), b"two").unwrap();
        std::fs::create_dir(root.path().join("dir.txt")).unwrap();
        let rows = list_checked_in(root.path(), "TMP LIST CAFÉ", || true).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, crate::plugins::tempfile::list_action(&root.path().join("Café.txt")));
        assert!(list_checked_in(root.path(), "tmp list", || false).is_err());
    }

    #[test]
    fn listed_file_association_checks_exact_identity_and_current_authority() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("owned.txt");
        std::fs::write(&path, b"original").unwrap();
        let row = list_checked_in(root.path(), "tmp list", || true).unwrap().remove(0);
        let calls = std::cell::Cell::new(0);
        let outcome = open_listed_with(&row, &|| true, |_, guard| {
            calls.set(calls.get() + 1);
            assert!(guard());
            Ok(())
        }).unwrap();
        assert!(matches!(outcome, CreateAndOpenOutcome::Opened(_)));
        assert_eq!(calls.get(), 1);
        let active = std::cell::Cell::new(true);
        let outcome = open_listed_with(&row, &|| active.get(), |_, guard| {
            active.set(false);
            assert!(!guard());
            anyhow::bail!("revoked during association")
        }).unwrap();
        assert!(matches!(outcome, CreateAndOpenOutcome::Published { reason: PublishedReason::Superseded, .. }));
        let moved = root.path().join("moved.txt");
        std::fs::rename(&path, moved).unwrap();
        std::fs::write(&path, b"replacement").unwrap();
        let outcome = open_listed_with(&row, &|| true, |_, _| panic!("replacement must not launch")).unwrap();
        assert!(matches!(outcome, CreateAndOpenOutcome::Published { reason: PublishedReason::Changed, .. }));
        let outcome = open_listed_with(&row, &|| false, |_, _| panic!("revoked request must not launch")).unwrap();
        assert!(matches!(outcome, CreateAndOpenOutcome::Published { reason: PublishedReason::Superseded, .. }));
    }

    #[test]
    fn listed_file_rejects_replaced_storage_root() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("storage");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("owned.txt"), b"original").unwrap();
        let row = list_checked_in(&root, "tmp list", || true).unwrap().remove(0);
        std::fs::rename(&root, base.path().join("moved")).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("owned.txt"), b"replacement").unwrap();
        let outcome = open_listed_with(&row, &|| true, |_, _| panic!("replacement root must not launch")).unwrap();
        assert!(matches!(outcome, CreateAndOpenOutcome::Published { reason: PublishedReason::Changed, .. }));
    }

    #[cfg(windows)]
    #[test]
    fn checked_alias_and_remove_preserve_siblings_and_refuse_collision() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("Café.txt");
        let sibling = root.path().join("other.txt");
        let target = root.path().join("temp_new name.txt");
        std::fs::write(&original, b"exact").unwrap();
        std::fs::write(&sibling, b"sibling").unwrap();
        let row = list_checked_in(root.path(), "tmp alias Café.txt new name", || true).unwrap().remove(0);
        let result = mutate_listed_with_effect(&row, || true, || {
            std::fs::write(&target, b"foreign").unwrap(); true
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"foreign");
        assert_eq!(std::fs::read(&original).unwrap(), b"exact");
        std::fs::remove_file(&target).unwrap();
        assert!(mutate_listed_with_effect(&row, || true, || false).is_err());
        assert!(original.exists());
        mutate_listed_with_effect(&row, || true, || true).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"exact");
        assert!(!original.exists());
        let row = list_checked_in(root.path(), "tmp rm new name", || true).unwrap().remove(0);
        mutate_listed_with_effect(&row, || true, || true).unwrap();
        assert!(!target.exists());
        assert_eq!(std::fs::read(&sibling).unwrap(), b"sibling");
    }

    #[cfg(windows)]
    #[test]
    fn checked_mutation_rejects_replaced_file_and_root() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("storage");
        std::fs::create_dir(&root).unwrap();
        let path = root.join("owned.txt");
        std::fs::write(&path, b"original").unwrap();
        let row = list_checked_in(&root, "tmp rm owned", || true).unwrap().remove(0);
        std::fs::rename(&path, root.join("moved.txt")).unwrap();
        std::fs::write(&path, b"replacement").unwrap();
        assert!(mutate_listed_with_effect(&row, || true, || true).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        std::fs::rename(&root, base.path().join("old")).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("owned.txt"), b"other").unwrap();
        assert!(mutate_listed_with_effect(&row, || true, || true).is_err());
        assert_eq!(std::fs::read(root.join("owned.txt")).unwrap(), b"other");
    }

    #[cfg(windows)]
    #[test]
    fn checked_alias_refuses_hard_link_and_same_name_is_guarded_noop() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("temp_owned.txt");
        let target = root.path().join("temp_other.txt");
        std::fs::write(&source, b"owned").unwrap();
        std::fs::hard_link(&source, &target).unwrap();
        let row = list_checked_in(root.path(), "tmp alias temp_owned.txt other", || true).unwrap().remove(0);
        let calls = std::cell::Cell::new(0);
        assert!(mutate_listed_with_effect(&row, || true, || { calls.set(calls.get() + 1); true }).is_err());
        assert_eq!(calls.get(), 0);
        assert_eq!(std::fs::read(&source).unwrap(), b"owned");
        let same = list_checked_in(root.path(), "tmp alias temp_owned.txt owned", || true).unwrap().remove(0);
        assert!(mutate_listed_with_effect(&same, || false, || { calls.set(calls.get() + 1); true }).is_err());
        assert_eq!(calls.get(), 0);
        mutate_listed_with_effect(&same, || true, || { calls.set(calls.get() + 1); true }).unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(std::fs::read(&source).unwrap(), b"owned");
    }

    #[cfg(windows)]
    #[test]
    fn checked_alias_rejects_hard_link_created_during_effect_claim() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("owned.txt");
        let target = root.path().join("temp_late.txt");
        std::fs::write(&source, b"original").unwrap();
        let row = list_checked_in(root.path(), "tmp alias owned.txt late", || true).unwrap().remove(0);
        let claims = std::cell::Cell::new(0);
        let result = mutate_listed_with_effect(&row, || true, || {
            claims.set(claims.get() + 1);
            std::fs::hard_link(&source, &target).unwrap();
            true
        });
        assert!(result.is_err());
        assert_eq!(claims.get(), 1);
        assert_eq!(std::fs::read(&source).unwrap(), b"original");
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
    }

    #[cfg(windows)]
    #[test]
    fn checked_alias_rechecks_authority_after_effect_claim() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("owned.txt");
        std::fs::write(&source, b"original").unwrap();
        let row = list_checked_in(root.path(), "tmp alias owned.txt later", || true).unwrap().remove(0);
        let current = std::cell::Cell::new(true);
        let claims = std::cell::Cell::new(0);
        let result = mutate_listed_with_effect(&row, || current.get(), || {
            claims.set(claims.get() + 1);
            current.set(false);
            true
        });
        assert!(result.is_err());
        assert_eq!(claims.get(), 1);
        assert_eq!(std::fs::read(&source).unwrap(), b"original");
        assert!(!root.path().join("temp_later.txt").exists());
    }

    #[cfg(windows)]
    #[test]
    fn checked_mutation_accepts_a_relative_storage_directory() {
        let root = tempfile::tempdir_in(".").unwrap();
        let relative = Path::new(root.path().file_name().unwrap());
        assert!(relative.is_relative());
        assert_eq!(std::fs::canonicalize(relative).unwrap(), std::fs::canonicalize(root.path()).unwrap());
        let source = relative.join("owned.txt");
        std::fs::write(&source, b"owned").unwrap();
        let row = list_checked_in(relative, "tmp rm owned", || true).unwrap().remove(0);
        mutate_listed_with_effect(&row, || true, || true).unwrap();
        assert!(!source.exists());
    }

    #[cfg(windows)]
    #[test]
    fn listed_association_pins_root_only_until_callback_settles() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("storage");
        let moved = base.path().join("moved");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("owned.txt"), b"original").unwrap();
        let row = list_checked_in(&root, "tmp list", || true).unwrap().remove(0);
        let outcome = open_listed_with(&row, &|| true, |_, guard| {
            assert!(guard());
            assert!(std::fs::rename(&root, &moved).is_err());
            Ok(())
        }).unwrap();
        assert!(matches!(outcome, CreateAndOpenOutcome::Opened(_)));
        std::fs::rename(&root, &moved).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn listed_files_exclude_symbolic_links() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("owned.txt"), b"data").unwrap();
        std::os::unix::fs::symlink(root.path().join("owned.txt"), root.path().join("alias.txt")).unwrap();
        let rows = list_checked_in(root.path(), "tmp list", || true).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action.label, "owned.txt");
    }

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
