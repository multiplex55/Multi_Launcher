//! Bounded, direct, read-only observations of a selected Downloads directory.
use super::*;
use crate::file_operations::{TransferEnumerationError, enumerate_transfer_directory_visit};

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

#[derive(Debug)]
pub(crate) enum DownloadsScanCompletion {
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
