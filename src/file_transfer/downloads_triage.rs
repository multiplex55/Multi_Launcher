//! Bounded, direct, read-only observations of a selected Downloads directory.
use super::*;
use crate::file_operations::{TransferEnumerationError, enumerate_transfer_directory_visit};

mod recycle;
pub(crate) use recycle::{MAX_RECYCLE_SELECTION, recycle_downloads};

const MAX_ROWS: usize = 10_000;
const MAX_NAME_BYTES: usize = MAX_ROWS * 512;
const MAX_ZONE_BYTES: usize = 16 * 1024;
const MAX_ZONE_TOTAL: usize = 512 * 1024;
const MAX_ZONE_PROBES: usize = 512;

/// Stable proof over the exact native leaf, full parent chain, identity and revision.
pub(crate) fn downloads_review_key(entry: &EntrySnapshot) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"MultiLauncher Downloads review proof v1\0");
    let units: Vec<u16> = entry.leaf.0.encode_wide().collect();
    hash.update((units.len() as u64).to_le_bytes());
    for unit in units {
        hash.update(unit.to_le_bytes());
    }
    hash.update((entry.parent.chain.len() as u64).to_le_bytes());
    for stamp in entry.parent.chain.iter() {
        hash.update(stamp.identity.volume_serial_number().to_le_bytes());
        hash.update(stamp.identity.file_id());
        hash.update(stamp.created.to_le_bytes());
    }
    hash.update(entry.identity.volume_serial_number().to_le_bytes());
    hash.update(entry.identity.file_id());
    let metadata = entry.metadata;
    hash.update([match metadata.kind {
        EntryKind::File => 0,
        EntryKind::Directory => 1,
    }]);
    match metadata.byte_len {
        Some(n) => {
            hash.update([1]);
            hash.update(n.to_le_bytes());
        }
        None => hash.update([0]),
    }
    hash.update(metadata.attributes.to_le_bytes());
    hash.update(metadata.created.to_le_bytes());
    hash.update(metadata.modified.to_le_bytes());
    hash.update(metadata.changed.to_le_bytes());
    hash.finalize().into()
}

#[derive(Debug, PartialEq, Eq)]
pub enum DownloadsScanCompletion {
    Complete,
    Limited,
    Cancelled,
    Io(String),
}
#[derive(Debug)]
pub(crate) struct DownloadsScanRow {
    pub name: OsString,
    pub entry: Result<EntrySnapshot, AdmissionError>,
    /// Bounded raw Zone.Identifier bytes, absent when unavailable or over budget.
    pub zone: Option<Vec<u8>>,
}
#[derive(Debug)]
pub(crate) struct DownloadsScan {
    pub root: ParentSnapshot,
    pub rows: Vec<DownloadsScanRow>,
    pub completion: DownloadsScanCompletion,
}

pub(crate) fn scan_downloads_directory(
    path: &Path,
    cancellation: &ScanCancellation,
) -> Result<DownloadsScan, AdmissionError> {
    scan_with_limits(path, cancellation, MAX_ROWS, MAX_NAME_BYTES)
}
fn scan_with_limits(
    path: &Path,
    cancellation: &ScanCancellation,
    max_rows: usize,
    max_name_bytes: usize,
) -> Result<DownloadsScan, AdmissionError> {
    let held = HeldParent::open_with_access(path, cancellation, DirectoryAccess::List)?;
    let root_metadata = observe_entry_metadata(held.directory().file())?;
    let mut rows = Vec::new();
    let mut stale_parent = false;
    let mut zone_bytes = 0usize;
    let mut zone_probes = 0usize;
    let io = crate::archive::transfer_native::TransferIo::new(cancellation.clone());
    let failure = enumerate_transfer_directory_visit(
        held.directory().file(),
        held.snapshot.path(),
        max_rows.min(MAX_ROWS),
        max_name_bytes.min(MAX_NAME_BYTES),
        || cancellation.is_cancelled(),
        |listed| {
            if cancellation.is_cancelled() || stale_parent {
                return;
            }
            let mut entry =
                inspect_listed_entry(&listed, |leaf| held.inspect_leaf(leaf, cancellation));
            if matches!(entry, Err(AdmissionError::Cancelled)) {
                return;
            }
            if matches!(entry, Err(AdmissionError::StaleParent)) {
                stale_parent = true;
                return;
            }
            let mut zone = None;
            if let Ok(snapshot) = &entry {
                if snapshot.metadata.kind == EntryKind::File
                    && zone_probes < MAX_ZONE_PROBES
                    && zone_bytes < MAX_ZONE_TOTAL
                {
                    zone_probes += 1;
                    if let Ok(stream) = held
                        .directory()
                        .open_source_zone(snapshot.leaf.as_os_str(), snapshot.identity)
                    {
                        if let Ok(metadata) = stream.file().metadata() {
                            let length = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
                            if length > 0
                                && length <= MAX_ZONE_BYTES
                                && length <= MAX_ZONE_TOTAL - zone_bytes
                            {
                                zone_bytes += length;
                                let mut buffer = Vec::with_capacity(length);
                                while buffer.len() < length && !cancellation.is_cancelled() {
                                    let remaining = length - buffer.len();
                                    match io.read_at(&stream, buffer.len() as u64, remaining) {
                                        Ok((data, count)) if count > 0 && count <= remaining => {
                                            buffer.extend_from_slice(&data[..count])
                                        }
                                        _ => break,
                                    }
                                }
                                if buffer.len() == length
                                    && stream
                                        .file()
                                        .metadata()
                                        .is_ok_and(|after| after.len() == length as u64)
                                {
                                    zone = Some(buffer);
                                }
                            }
                        }
                    }
                    match held.inspect_leaf(&snapshot.leaf, cancellation) {
                        Ok(TargetOccupancy::Occupied(current)) if current == *snapshot => {}
                        Err(AdmissionError::Cancelled) => return,
                        Err(AdmissionError::StaleParent) => {
                            stale_parent = true;
                            return;
                        }
                        _ => {
                            zone = None;
                            entry = Err(AdmissionError::StaleEntry);
                        }
                    }
                }
            }
            rows.push(DownloadsScanRow {
                name: listed.name,
                entry,
                zone,
            });
        },
    );
    if stale_parent {
        return Err(AdmissionError::StaleParent);
    }
    // Always recheck the held chain, even when cancellation ended the scan.
    held.check_chain(&ScanCancellation::new())?;
    if observe_entry_metadata(held.directory().file())? != root_metadata {
        return Err(AdmissionError::StaleParent);
    }
    let completion = if cancellation.is_cancelled() {
        DownloadsScanCompletion::Cancelled
    } else {
        match failure {
            None => DownloadsScanCompletion::Complete,
            Some(TransferEnumerationError::Limit) => DownloadsScanCompletion::Limited,
            Some(TransferEnumerationError::Cancelled) => DownloadsScanCompletion::Cancelled,
            Some(TransferEnumerationError::Io(message)) => DownloadsScanCompletion::Io(message),
        }
    };
    Ok(DownloadsScan {
        root: held.snapshot,
        rows,
        completion,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn downloads_triage_shallow_and_bounded_prefix() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("one.txt"), b"a").unwrap();
        std::fs::write(temp.path().join("two.txt"), b"b").unwrap();
        std::fs::create_dir(temp.path().join("folder")).unwrap();
        let cancel = ScanCancellation::new();
        let scan = scan_downloads_directory(temp.path(), &cancel).unwrap();
        assert!(matches!(scan.completion, DownloadsScanCompletion::Complete));
        assert_eq!(scan.rows.len(), 3);
        assert!(scan.rows.iter().all(|row| row.entry.is_ok()));
        let limited = scan_with_limits(temp.path(), &cancel, 1, MAX_NAME_BYTES).unwrap();
        assert!(matches!(
            limited.completion,
            DownloadsScanCompletion::Limited
        ));
        assert_eq!(limited.rows.len(), 1);
        let held =
            HeldParent::open_with_access(temp.path(), &cancel, DirectoryAccess::List).unwrap();
        assert!(matches!(
            enumerate_transfer_directory(
                held.directory().file(),
                held.snapshot.path(),
                1,
                MAX_NAME_BYTES,
                || false
            ),
            Err(TransferEnumerationError::Limit)
        ));
    }
    #[test]
    fn downloads_triage_native_zone_and_review_key_change() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("download.exe");
        std::fs::write(&path, b"first").unwrap();
        let mut zone = path.as_os_str().to_os_string();
        zone.push(":Zone.Identifier:$DATA");
        let contents = b"[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.com/file\r\n";
        std::fs::write(&zone, contents).unwrap();
        let cancellation = ScanCancellation::new();
        let scan = scan_downloads_directory(temp.path(), &cancellation).unwrap();
        let row = scan
            .rows
            .iter()
            .find(|row| row.name == "download.exe")
            .unwrap();
        assert_eq!(row.zone.as_deref(), Some(contents.as_slice()));
        let before = downloads_review_key(row.entry.as_ref().unwrap());
        std::fs::write(&path, b"changed length").unwrap();
        let after = scan_downloads_directory(temp.path(), &cancellation).unwrap();
        let row = after
            .rows
            .iter()
            .find(|row| row.name == "download.exe")
            .unwrap();
        assert_ne!(before, downloads_review_key(row.entry.as_ref().unwrap()));
    }
    #[test]
    fn downloads_triage_placeholder_skips_open() {
        let entry = TransferDirectoryEntry {
            name: "cloud".into(),
            kind: TransferDirectoryEntryKind::CloudPlaceholder,
        };
        assert!(matches!(
            inspect_listed_entry(&entry, |_| panic!("cloud entry opened")),
            Err(AdmissionError::CloudPlaceholder)
        ));
    }
}

mod guarded {
    //! Guarded actions on explicitly selected Downloads entries.
    use super::super::{
        AdmissionError, DirectoryAccess, EntryKind, EntrySnapshot, HeldParent, TargetOccupancy,
        TransferLeaf, check_cancel, check_target_bounds,
    };
    use crate::archive::transfer_native::{TransferFile, TransferIo};
    use crate::file_operations::ScanCancellation;
    use anyhow::{Context, bail};
    use sha2::{Digest, Sha256};
    use std::path::{Path, PathBuf};

    const MAX_COMPARE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
    const CHUNK: usize = 256 * 1024;

    fn parent(
        entry: &EntrySnapshot,
        cancel: &ScanCancellation,
        access: DirectoryAccess,
    ) -> anyhow::Result<HeldParent> {
        let parent = HeldParent::open_with_access(entry.parent().path(), cancel, access)?;
        if parent.snapshot != *entry.parent() {
            return Err(AdmissionError::StaleParent.into());
        }
        Ok(parent)
    }
    fn checked(
        parent: &HeldParent,
        file: &std::fs::File,
        entry: &EntrySnapshot,
        cancel: &ScanCancellation,
    ) -> anyhow::Result<()> {
        parent.check_chain(cancel)?;
        if parent.capture_entry(file, entry.leaf())? != *entry {
            return Err(AdmissionError::StaleEntry.into());
        }
        Ok(())
    }

    /// Holds the exact entry and its ancestor chain against deletion during the callback.
    pub fn with_downloads_entry_path<T>(
        entry: &EntrySnapshot,
        cancel: &ScanCancellation,
        action: impl FnOnce(&Path) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let parent = parent(entry, cancel, DirectoryAccess::Observe)?;
        match entry.metadata().kind {
            EntryKind::File => {
                let file = parent
                    .directory()
                    .open_read(entry.leaf().as_os_str())
                    .map_err(AdmissionError::from)?;
                checked(&parent, file.file(), entry, cancel)?;
                action(&entry.path())
            }
            EntryKind::Directory => {
                let dir = parent
                    .directory()
                    .open_child(entry.leaf().as_os_str())
                    .map_err(AdmissionError::from)?;
                checked(&parent, dir.file(), entry, cancel)?;
                action(&entry.path())
            }
        }
    }

    /// Renames the held exact entry into a vacant sibling without replacing a name.
    pub fn rename_downloads_entry(
        entry: &EntrySnapshot,
        name: &TransferLeaf,
        cancel: &ScanCancellation,
    ) -> anyhow::Result<PathBuf> {
        let access = match entry.metadata().kind {
            EntryKind::File => DirectoryAccess::CreateFile,
            EntryKind::Directory => DirectoryAccess::CreateDirectory,
        };
        let parent = parent(entry, cancel, access)?;
        check_target_bounds(&parent.snapshot, name)?;
        if !matches!(parent.inspect_leaf(name, cancel)?, TargetOccupancy::Vacant) {
            bail!("The new name is already occupied.");
        }
        let destination = parent.snapshot.path().join(name.as_os_str());
        match entry.metadata().kind {
            EntryKind::File => {
                let held = parent
                    .directory()
                    .open_relocation(entry.leaf().as_os_str(), entry.identity())
                    .map_err(AdmissionError::from)?;
                checked(&parent, held.file(), entry, cancel)?;
                check_cancel(cancel)?;
                held.rename_no_replace(parent.directory(), name.as_os_str())
                    .map_err(AdmissionError::from)
                    .context("Rename could not be confirmed; inspect both names before retrying")?;
                let result = parent.capture_entry(held.file(), name)?;
                if result.identity() != entry.identity()
                    || result.leaf().as_os_str() != name.as_os_str()
                {
                    bail!(
                        "Rename destination could not be confirmed: {}",
                        destination.display()
                    );
                }
            }
            EntryKind::Directory => {
                let held = parent
                    .directory()
                    .open_removable_directory_child(entry.leaf().as_os_str(), entry.identity())
                    .map_err(AdmissionError::from)?;
                checked(&parent, held.file(), entry, cancel)?;
                check_cancel(cancel)?;
                held.rename_no_replace(parent.directory(), name.as_os_str())
                    .map_err(AdmissionError::from)
                    .context("Rename could not be confirmed; inspect both names before retrying")?;
                let result = parent.capture_entry(held.file(), name)?;
                if result.identity() != entry.identity()
                    || result.leaf().as_os_str() != name.as_os_str()
                {
                    bail!(
                        "Rename destination could not be confirmed: {}",
                        destination.display()
                    );
                }
            }
        }
        parent.check_chain(&ScanCancellation::new())?;
        Ok(destination)
    }

    fn digest(
        file: &TransferFile,
        len: u64,
        io: &TransferIo,
        cancel: &ScanCancellation,
    ) -> anyhow::Result<[u8; 32]> {
        let mut hash = Sha256::new();
        let mut offset = 0u64;
        while offset < len {
            check_cancel(cancel)?;
            let size = (len - offset).min(CHUNK as u64) as usize;
            let (bytes, read) = io.read_at(file, offset, size)?;
            if read == 0 || read > size {
                bail!("File changed during comparison.");
            }
            hash.update(&bytes[..read]);
            offset = offset
                .checked_add(read as u64)
                .context("File length overflow")?;
        }
        Ok(hash.finalize().into())
    }
    /// Compares only the selected files' main streams, under exact held read guards.
    pub fn compare_downloads_pair(
        first: &EntrySnapshot,
        second: &EntrySnapshot,
        cancel: &ScanCancellation,
    ) -> anyhow::Result<bool> {
        if first.metadata().kind != EntryKind::File
            || second.metadata().kind != EntryKind::File
            || first.identity() == second.identity()
        {
            bail!("Choose two distinct regular files.");
        }
        let length = first.metadata().byte_len.context("Missing file length")?;
        if second.metadata().byte_len != Some(length) {
            bail!("Files must have equal lengths.");
        }
        if length > MAX_COMPARE_BYTES {
            bail!("File exceeds the 2 GiB comparison limit.");
        }
        let a = parent(first, cancel, DirectoryAccess::Observe)?;
        let b = parent(second, cancel, DirectoryAccess::Observe)?;
        let fa = a
            .directory()
            .open_read(first.leaf().as_os_str())
            .map_err(AdmissionError::from)?;
        let fb = b
            .directory()
            .open_read(second.leaf().as_os_str())
            .map_err(AdmissionError::from)?;
        checked(&a, fa.file(), first, cancel)?;
        checked(&b, fb.file(), second, cancel)?;
        let io = TransferIo::new(cancel.clone());
        let equal = digest(&fa, length, &io, cancel)? == digest(&fb, length, &io, cancel)?;
        checked(&a, fa.file(), first, cancel)?;
        checked(&b, fb.file(), second, cancel)?;
        Ok(equal)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::file_transfer::inspect_source;
        use std::{ffi::OsStr, fs};
        #[test]
        fn downloads_triage_compare_and_stale_selection() {
            let tmp = tempfile::tempdir().unwrap();
            let a = tmp.path().join("a");
            let b = tmp.path().join("b");
            let c = tmp.path().join("c");
            fs::write(&a, b"abc").unwrap();
            fs::write(&b, b"abc").unwrap();
            fs::write(&c, b"xyz").unwrap();
            let cancel = ScanCancellation::new();
            let sa = inspect_source(&a, &cancel).unwrap();
            let sb = inspect_source(&b, &cancel).unwrap();
            let sc = inspect_source(&c, &cancel).unwrap();
            assert_eq!(
                with_downloads_entry_path(&sa, &cancel, |path| Ok(path.to_owned())).unwrap(),
                sa.path()
            );
            assert!(compare_downloads_pair(&sa, &sb, &cancel).unwrap());
            assert!(!compare_downloads_pair(&sa, &sc, &cancel).unwrap());
            fs::write(&b, b"changed").unwrap();
            assert!(compare_downloads_pair(&sa, &sb, &cancel).is_err());
        }
        #[test]
        fn downloads_triage_rename_never_replaces_and_moves_directory() {
            let tmp = tempfile::tempdir().unwrap();
            let a = tmp.path().join("a");
            let b = tmp.path().join("b");
            let dir = tmp.path().join("dir");
            fs::write(&a, b"a").unwrap();
            fs::write(&b, b"b").unwrap();
            fs::create_dir(&dir).unwrap();
            let cancel = ScanCancellation::new();
            let a_snapshot = inspect_source(&a, &cancel).unwrap();
            assert!(
                rename_downloads_entry(
                    &a_snapshot,
                    &TransferLeaf::new(OsStr::new("b")).unwrap(),
                    &cancel
                )
                .is_err()
            );
            assert_eq!(fs::read(&b).unwrap(), b"b");
            let renamed = rename_downloads_entry(
                &a_snapshot,
                &TransferLeaf::new(OsStr::new("c")).unwrap(),
                &cancel,
            )
            .unwrap();
            assert_eq!(fs::read(renamed).unwrap(), b"a");
            let dir_snapshot = inspect_source(&dir, &cancel).unwrap();
            let renamed = rename_downloads_entry(
                &dir_snapshot,
                &TransferLeaf::new(OsStr::new("dir2")).unwrap(),
                &cancel,
            )
            .unwrap();
            assert!(renamed.is_dir());
        }
    }
}
pub use guarded::{compare_downloads_pair, rename_downloads_entry, with_downloads_entry_path};
