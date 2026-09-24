use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, GetFinalPathNameByHandleW,
    VOLUME_NAME_DOS,
};

pub(super) const MAX_PROFILE_FILES: usize = 4_096;
pub(super) const MAX_PROFILE_DIRECTORIES: usize = 512;
pub(super) const MAX_PROFILE_DEPTH: usize = 32;
pub(super) const MAX_PROFILE_FILE_BYTES: u64 = 512 * 1024 * 1024;
pub(super) const MAX_PROFILE_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
enum EntryKind {
    Directory,
    File { bytes: u64, sha256: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    relative: PathBuf,
    kind: EntryKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProfileInventory {
    pub root: PathBuf,
    entries: Vec<Entry>,
    pub file_count: usize,
    pub directory_count: usize,
    pub total_bytes: u64,
    pub tree_sha256: String,
}

impl ProfileInventory {
    pub(super) fn scan(path: &Path) -> Result<Self, String> {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("inspect supplied profile directory: {error}"))?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            return Err(
                "profile copy source must be a regular directory without reparse points".into(),
            );
        }
        let root = canonical_profile_root(path)?;
        let mut scanner = Scanner::default();
        scanner.walk(&root, Path::new(""), 0)?;
        verify_profile_directory(&root, &root, Path::new(""))?;
        scanner.entries.sort_by(|left, right| {
            portable_relative(&left.relative).cmp(&portable_relative(&right.relative))
        });
        let tree_sha256 = hash_entries(&scanner.entries);
        Ok(Self {
            root,
            entries: scanner.entries,
            file_count: scanner.file_count,
            directory_count: scanner.directory_count,
            total_bytes: scanner.total_bytes,
            tree_sha256,
        })
    }

    pub(super) fn copy_to(&self, destination: &Path) -> Result<Self, String> {
        let current = Self::scan(&self.root)?;
        if current != *self {
            return Err("profile source changed after its initial inventory".into());
        }
        let destination_metadata = fs::symlink_metadata(destination)
            .map_err(|error| format!("inspect isolated profile destination: {error}"))?;
        if !destination_metadata.is_dir() || is_reparse_point(&destination_metadata) {
            return Err(
                "isolated profile destination must be a regular temporary directory".into(),
            );
        }
        if fs::read_dir(destination)
            .map_err(|error| format!("inspect isolated profile destination: {error}"))?
            .next()
            .is_some()
        {
            return Err("isolated profile destination must be empty".into());
        }

        for entry in self
            .entries
            .iter()
            .filter(|entry| matches!(&entry.kind, EntryKind::Directory))
        {
            let target = destination.join(&entry.relative);
            fs::create_dir(&target)
                .map_err(|error| format!("create isolated profile directory entry: {error}"))?;
        }

        for entry in self
            .entries
            .iter()
            .filter(|entry| matches!(&entry.kind, EntryKind::File { .. }))
        {
            let (expected_bytes, expected_hash) = match &entry.kind {
                EntryKind::File { bytes, sha256 } => (*bytes, sha256.as_str()),
                EntryKind::Directory => unreachable!(),
            };
            let source = self.root.join(&entry.relative);
            let before = fs::symlink_metadata(&source)
                .map_err(|error| format!("recheck profile source entry before copy: {error}"))?;
            if !before.is_file() || is_reparse_point(&before) || before.len() != expected_bytes {
                return Err("profile source entry changed type or size during copy".into());
            }
            let mut input = open_regular_profile_file(&source, &self.root, &entry.relative)
                .map_err(|error| format!("open inventoried profile source entry: {error}"))?;
            let opened = input
                .metadata()
                .map_err(|error| format!("inspect opened profile source entry: {error}"))?;
            if !opened.is_file() || is_reparse_point(&opened) || opened.len() != expected_bytes {
                return Err("opened profile source entry changed while copying".into());
            }
            let target = destination.join(&entry.relative);
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|error| format!("create isolated profile file: {error}"))?;
            let copied_hash = copy_and_hash(&mut input, &mut output)
                .map_err(|error| format!("copy profile file contents: {error}"))?;
            if copied_hash != expected_hash {
                return Err(
                    "copied profile file did not match the inventoried source bytes".into(),
                );
            }
        }

        if Self::scan(&self.root)? != *self {
            return Err("profile source changed while its isolated copy was being created".into());
        }
        let copied = Self::scan(destination)?;
        if copied.file_count != self.file_count
            || copied.directory_count != self.directory_count
            || copied.total_bytes != self.total_bytes
            || copied.tree_sha256 != self.tree_sha256
        {
            return Err("isolated profile copy did not match the initial source inventory".into());
        }
        Ok(copied)
    }

    #[cfg(test)]
    pub(super) fn still_matches_source(&self) -> bool {
        Self::scan(&self.root).is_ok_and(|current| current == *self)
    }

    pub(super) fn file_hash(&self, file_name: &str) -> Option<&str> {
        self.entries.iter().find_map(|entry| {
            (entry.relative == Path::new(file_name))
                .then(|| match &entry.kind {
                    EntryKind::File { sha256, .. } => Some(sha256.as_str()),
                    EntryKind::Directory => None,
                })
                .flatten()
        })
    }

    pub(super) fn file_count_and_bytes(&self) -> (usize, u64) {
        (self.file_count, self.total_bytes)
    }
}

#[derive(Default)]
struct Scanner {
    entries: Vec<Entry>,
    file_count: usize,
    directory_count: usize,
    total_bytes: u64,
    casefold_paths: BTreeMap<String, PathBuf>,
}

impl Scanner {
    fn walk(&mut self, root: &Path, relative: &Path, depth: usize) -> Result<(), String> {
        if depth > MAX_PROFILE_DEPTH {
            return Err(format!(
                "profile directory depth exceeds {MAX_PROFILE_DEPTH}"
            ));
        }
        let directory = root.join(relative);
        verify_profile_directory(&directory, root, relative)?;
        for child in fs::read_dir(&directory)
            .map_err(|error| format!("enumerate profile directory entry: {error}"))?
        {
            let child = child.map_err(|error| format!("read profile directory entry: {error}"))?;
            let name = child.file_name();
            let name_text = name.to_str().ok_or_else(|| {
                "profile contains a path component without valid UTF-8/Unicode".to_string()
            })?;
            if name_text.is_empty()
                || name_text.contains(['/', '\\', ':', '\0'])
                || name_text.chars().any(char::is_control)
            {
                return Err("profile contains a path component unsafe for isolated copying".into());
            }
            let mut child_relative = relative.to_path_buf();
            child_relative.push(&name);
            if child_relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err("profile entry contains an unsafe traversal component".into());
            }
            let portable = portable_relative(&child_relative);
            let collision_key = portable.to_lowercase();
            if let Some(previous) = self
                .casefold_paths
                .insert(collision_key, child_relative.clone())
            {
                return Err(format!(
                    "profile contains case-insensitive path collisions at depths {} and {}",
                    previous.components().count(),
                    child_relative.components().count()
                ));
            }
            let metadata = fs::symlink_metadata(child.path())
                .map_err(|error| format!("inspect profile entry type: {error}"))?;
            if is_reparse_point(&metadata) {
                return Err("profile contains a symlink or reparse point".into());
            }
            if metadata.is_dir() {
                self.directory_count = self.directory_count.saturating_add(1);
                if self.directory_count > MAX_PROFILE_DIRECTORIES {
                    return Err(format!(
                        "profile directory count exceeds {MAX_PROFILE_DIRECTORIES}"
                    ));
                }
                self.entries.push(Entry {
                    relative: child_relative.clone(),
                    kind: EntryKind::Directory,
                });
                self.walk(root, &child_relative, depth + 1)?;
            } else if metadata.is_file() {
                self.file_count = self.file_count.saturating_add(1);
                if self.file_count > MAX_PROFILE_FILES {
                    return Err(format!("profile file count exceeds {MAX_PROFILE_FILES}"));
                }
                if metadata.len() > MAX_PROFILE_FILE_BYTES {
                    return Err(format!(
                        "profile file exceeds the {} byte per-file limit",
                        MAX_PROFILE_FILE_BYTES
                    ));
                }
                self.total_bytes = self
                    .total_bytes
                    .checked_add(metadata.len())
                    .ok_or_else(|| "profile total byte count overflowed".to_string())?;
                if self.total_bytes > MAX_PROFILE_TOTAL_BYTES {
                    return Err(format!(
                        "profile exceeds the {} byte total limit",
                        MAX_PROFILE_TOTAL_BYTES
                    ));
                }
                let mut opened = open_regular_profile_file(&child.path(), root, &child_relative)
                    .map_err(|error| format!("open profile file for hashing: {error}"))?;
                let digest = hash_open_file(&mut opened)
                    .map_err(|error| format!("hash opened regular profile file: {error}"))?;
                let after = fs::symlink_metadata(child.path())
                    .map_err(|error| format!("recheck profile file after hashing: {error}"))?;
                if !after.is_file() || is_reparse_point(&after) || after.len() != metadata.len() {
                    return Err("profile file changed type or size while hashing".into());
                }
                self.entries.push(Entry {
                    relative: child_relative,
                    kind: EntryKind::File {
                        bytes: metadata.len(),
                        sha256: digest,
                    },
                });
            } else {
                return Err("profile contains a non-regular file system entry".into());
            }
        }
        verify_profile_directory(&directory, root, relative)?;
        Ok(())
    }
}

pub(super) fn ensure_no_overlap(source: &Path, targets: &[&Path]) -> Result<(), String> {
    let source = canonical_profile_root(source)?;
    let source = normalized_absolute(&source)?;
    for target in targets {
        let target = resolve_future_path(target)?;
        if paths_overlap(&source, &target) {
            return Err("profile source overlaps an output, report, or temporary directory".into());
        }
    }
    Ok(())
}

pub(super) fn resolve_future_path(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("resolve working directory: {error}"))?
            .join(path)
    };
    let mut existing = absolute.as_path();
    let mut tail = Vec::new();
    loop {
        match fs::symlink_metadata(existing) {
            Ok(metadata) => {
                if is_reparse_point(&metadata) {
                    return Err("an output path component is a symlink or reparse point".into());
                }
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("inspect output path component: {error}")),
        }
        let name = existing
            .file_name()
            .ok_or_else(|| "could not find an existing parent for an output path".to_string())?;
        tail.push(name.to_os_string());
        existing = existing
            .parent()
            .ok_or_else(|| "could not find an existing parent for an output path".to_string())?;
    }
    let mut resolved = existing
        .canonicalize()
        .map_err(|error| format!("canonicalize existing output parent: {error}"))?;
    for part in tail.into_iter().rev() {
        resolved.push(part);
    }
    Ok(normalized_absolute(&resolved)?)
}

fn normalized_absolute(path: &Path) -> Result<PathBuf, String> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err("absolute path escapes its root during normalization".into());
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    Ok(normalized)
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    let left = portable_absolute(left).to_lowercase();
    let right = portable_absolute(right).to_lowercase();
    left == right
        || left
            .strip_prefix(&right)
            .is_some_and(|suffix| suffix.starts_with('\\'))
        || right
            .strip_prefix(&left)
            .is_some_and(|suffix| suffix.starts_with('\\'))
}

fn portable_absolute(path: &Path) -> String {
    let portable = path.to_string_lossy().replace('/', "\\");
    #[cfg(windows)]
    {
        if let Some(unc) = portable.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{unc}");
        }
        if let Some(drive_path) = portable.strip_prefix(r"\\?\") {
            return drive_path.to_string();
        }
    }
    portable
}

fn portable_relative(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}

fn hash_entries(entries: &[Entry]) -> String {
    let mut digest = Sha256::new();
    for entry in entries {
        let path = portable_relative(&entry.relative);
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path.as_bytes());
        match &entry.kind {
            EntryKind::Directory => digest.update([b'D']),
            EntryKind::File { bytes, sha256 } => {
                digest.update([b'F']);
                digest.update(bytes.to_le_bytes());
                digest.update(sha256.as_bytes());
            }
        }
    }
    hex::encode(digest.finalize())
}

fn hash_open_file(input: &mut File) -> std::io::Result<String> {
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

#[cfg(test)]
fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut input = File::open(path)?;
    hash_open_file(&mut input)
}

pub(super) fn canonical_profile_root(path: &Path) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let directory = open_profile_directory(path).map_err(|error| {
            format!("open supplied profile directory without following reparse points: {error}")
        })?;
        let metadata = directory
            .metadata()
            .map_err(|error| format!("inspect opened supplied profile directory: {error}"))?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            return Err(
                "profile copy source must be a regular directory without reparse points".into(),
            );
        }
        let final_path = final_path_for_handle(&directory)
            .map_err(|error| format!("resolve opened supplied profile directory: {error}"))?;
        return Ok(final_path);
    }
    #[cfg(not(windows))]
    {
        path.canonicalize()
            .map_err(|error| format!("resolve supplied profile directory: {error}"))
    }
}

fn verify_profile_directory(path: &Path, root: &Path, relative: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let directory = open_profile_directory(path).map_err(|error| {
            format!("open inventoried directory without following reparse points: {error}")
        })?;
        let metadata = directory
            .metadata()
            .map_err(|error| format!("inspect opened inventoried directory: {error}"))?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            return Err("profile directory changed to a non-directory or reparse point".into());
        }
        let actual = final_path_for_handle(&directory)
            .map_err(|error| format!("resolve opened inventoried directory: {error}"))?;
        let expected = root.join(relative);
        if !same_normalized_path(&actual, &expected) {
            return Err("profile directory resolved outside its inventoried source path".into());
        }
    }
    #[cfg(not(windows))]
    {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("inspect inventoried directory: {error}"))?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            return Err("profile directory changed to a non-directory or symlink".into());
        }
    }
    Ok(())
}

pub(super) fn open_regular_profile_file(
    path: &Path,
    root: &Path,
    relative: &Path,
) -> std::io::Result<File> {
    #[cfg(windows)]
    {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || is_reparse_point(&metadata) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "opened source entry is not a regular non-reparse file",
            ));
        }
        let actual = final_path_for_handle(&file)?;
        let expected = root.join(relative);
        if !same_normalized_path(&actual, &expected) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "opened source file resolved outside the inventoried profile path",
            ));
        }
        Ok(file)
    }
    #[cfg(not(windows))]
    {
        let before = fs::symlink_metadata(path)?;
        if !before.is_file() || is_reparse_point(&before) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "source entry is not a regular file",
            ));
        }
        let file = File::open(path)?;
        let after = file.metadata()?;
        if !after.is_file() || is_reparse_point(&after) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "opened source entry is not a regular file",
            ));
        }
        let _ = (root, relative);
        Ok(file)
    }
}

#[cfg(windows)]
fn open_profile_directory(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)
}

#[cfg(windows)]
fn final_path_for_handle(file: &File) -> std::io::Result<PathBuf> {
    let handle = HANDLE(file.as_raw_handle() as *mut std::ffi::c_void);
    let mut buffer = vec![0_u16; 512];
    loop {
        let length = unsafe { GetFinalPathNameByHandleW(handle, &mut buffer, VOLUME_NAME_DOS) };
        if length == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let length = length as usize;
        if length < buffer.len() {
            buffer.truncate(length);
            let path = String::from_utf16(&buffer)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
            return Ok(strip_windows_extended_prefix(path));
        }
        buffer.resize(length.saturating_add(1), 0);
    }
}

#[cfg(windows)]
fn strip_windows_extended_prefix(path: String) -> PathBuf {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(drive_path) = path.strip_prefix(r"\\?\") {
        PathBuf::from(drive_path)
    } else {
        PathBuf::from(path)
    }
}

fn same_normalized_path(left: &Path, right: &Path) -> bool {
    portable_absolute(left)
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(portable_absolute(right).trim_end_matches('\\'))
}

fn copy_and_hash(input: &mut File, output: &mut File) -> std::io::Result<String> {
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
        digest.update(&buffer[..read]);
    }
    output.flush()?;
    Ok(hex::encode(digest.finalize()))
}

#[cfg(windows)]
fn is_reparse_point(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_copy_preserves_exact_initial_bytes_and_tree_hash() {
        let source = tempfile::tempdir().unwrap();
        fs::create_dir(source.path().join("nested")).unwrap();
        fs::write(source.path().join("settings.json"), b"{\"ok\":true}").unwrap();
        fs::write(source.path().join("nested/radial.json"), b"{}\n").unwrap();
        let inventory = ProfileInventory::scan(source.path()).unwrap();
        let destination = tempfile::tempdir().unwrap();
        let copied = inventory.copy_to(destination.path()).unwrap();
        assert_eq!(copied.tree_sha256, inventory.tree_sha256);
        assert_eq!(
            copied.file_hash("settings.json"),
            inventory.file_hash("settings.json")
        );
        assert_eq!(
            fs::read(destination.path().join("settings.json")).unwrap(),
            b"{\"ok\":true}"
        );
        assert!(inventory.still_matches_source());
        fs::write(destination.path().join("settings.json"), b"changed").unwrap();
        assert!(inventory.still_matches_source());
    }

    #[cfg(not(windows))]
    #[test]
    fn profile_copy_rejects_case_insensitive_collisions() {
        let collision = tempfile::tempdir().unwrap();
        fs::write(collision.path().join("Settings.json"), b"a").unwrap();
        fs::write(collision.path().join("settings.json"), b"b").unwrap();
        assert!(
            ProfileInventory::scan(collision.path())
                .unwrap_err()
                .contains("case-insensitive")
        );
    }

    #[test]
    fn profile_copy_rejects_symlinks_and_reparse_points() {
        let source = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), source.path().join("linked")).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(outside.path(), source.path().join("linked")).is_err()
        {
            return;
        }
        assert!(
            ProfileInventory::scan(source.path())
                .unwrap_err()
                .contains("reparse")
        );
    }

    #[test]
    fn copy_rechecks_source_hashes_and_only_copy_mutations_change_copy_hashes() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("settings.json"), b"original").unwrap();
        let inventory = ProfileInventory::scan(source.path()).unwrap();
        let destination = tempfile::tempdir().unwrap();
        fs::write(source.path().join("settings.json"), b"replaced").unwrap();
        assert!(inventory.copy_to(destination.path()).is_err());

        let fresh = ProfileInventory::scan(source.path()).unwrap();
        let copy_directory = tempfile::tempdir().unwrap();
        let copied = fresh.copy_to(copy_directory.path()).unwrap();
        assert_eq!(fresh.tree_sha256, copied.tree_sha256);
        fs::write(
            copy_directory.path().join("settings.json"),
            b"normalized copy",
        )
        .unwrap();
        assert!(fresh.still_matches_source());
        assert_ne!(
            hash_file(&copy_directory.path().join("settings.json")).unwrap(),
            fresh.file_hash("settings.json").unwrap()
        );
    }

    #[test]
    fn profile_copy_rejects_non_regular_entries() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            use std::os::unix::net::UnixListener;

            let source = tempfile::tempdir().unwrap();
            let socket_path = source.path().join("socket");
            let _socket = UnixListener::bind(&socket_path).unwrap();
            assert!(
                ProfileInventory::scan(source.path())
                    .unwrap_err()
                    .contains("non-regular")
            );

            let target = tempfile::NamedTempFile::new().unwrap();
            symlink(target.path(), source.path().join("linked")).unwrap();
            assert!(ProfileInventory::scan(source.path()).is_err());
        }
    }

    #[test]
    fn overlap_checks_resolve_nonexistent_output_under_canonical_parent() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("profile");
        fs::create_dir(&source).unwrap();
        let overlapping_future = source.join("future-output").join("run");
        assert!(
            ensure_no_overlap(&source, &[&overlapping_future]).is_err(),
            "source={:?} canonical={:?} output={:?} resolved={:?}",
            source,
            canonical_profile_root(&source),
            overlapping_future,
            resolve_future_path(&overlapping_future),
        );
        let independent = root.path().join("other").join("run");
        assert!(ensure_no_overlap(&source, &[&independent]).is_ok());
    }

    #[test]
    fn profile_inventory_enforces_explicit_size_limits() {
        assert_eq!(MAX_PROFILE_FILES, 4_096);
        assert!(MAX_PROFILE_FILE_BYTES >= 85_287_936);
        assert!(MAX_PROFILE_TOTAL_BYTES >= 160_145_442);
    }
}
