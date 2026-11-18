//! Immutable display-only queue generations. They carry no execution authority.
use super::*;
use crate::file_transfer::TransferReceiptId;

const MAX_ROWS: usize = 32;
const MAX_TEXT: usize = 2048;
const MAGIC: &[u8; 8] = b"MLQUEUE1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::file_transfer) enum QueueAction {
    Copy,
    Move,
    Duplicate,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::file_transfer) struct QueuePreview {
    pub receipt_id: TransferReceiptId,
    pub action: QueueAction,
    pub source: String,
    pub destination: String,
    pub item_count: u64,
    pub verify_requested: bool,
    pub truncated: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::file_transfer) struct QueueSnapshotRead {
    pub rows: Vec<QueuePreview>,
    pub newer_incomplete: bool,
}

fn slot_name(generation: u64, nonce: [u8; 16]) -> OsString {
    let hex: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
    OsString::from(format!("queue-{generation:016x}-{hex}.snapshot"))
}
pub(super) fn parse_slot_name(name: &OsStr) -> Option<(u64, [u8; 16])> {
    let text = name.to_str()?;
    let rest = text.strip_prefix("queue-")?.strip_suffix(".snapshot")?;
    let (generation, nonce) = rest.split_once('-')?;
    if generation.len() != 16
        || nonce.len() != 32
        || !generation
            .bytes()
            .chain(nonce.bytes())
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return None;
    }
    let generation = u64::from_str_radix(generation, 16).ok()?;
    let mut id = [0; 16];
    for (i, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&nonce[i * 2..i * 2 + 2], 16).ok()?;
    }
    if generation == 0 || id == [0; 16] {
        return None;
    }
    Some((generation, id))
}
fn encode(
    generation: u64,
    nonce: [u8; 16],
    rows: &[QueuePreview],
) -> Result<Vec<u8>, ReceiptStoreError> {
    if rows.len() > MAX_ROWS {
        return Err(ReceiptStoreError::Capacity);
    }
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&generation.to_le_bytes());
    out.extend_from_slice(&nonce);
    out.push(rows.len() as u8);
    let mut ids = std::collections::HashSet::new();
    for row in rows {
        if row.receipt_id.0 == [0; 16]
            || !ids.insert(row.receipt_id.0)
            || row.source.len() > MAX_TEXT
            || row.destination.len() > MAX_TEXT
        {
            return Err(ReceiptStoreError::InvalidRecord);
        }
        out.extend_from_slice(&row.receipt_id.0);
        out.push(match row.action {
            QueueAction::Copy => 0,
            QueueAction::Move => 1,
            QueueAction::Duplicate => 2,
        });
        out.push(u8::from(row.verify_requested) | (u8::from(row.truncated) << 1));
        out.extend_from_slice(&row.item_count.to_le_bytes());
        for value in [&row.source, &row.destination] {
            out.extend_from_slice(&(value.len() as u16).to_le_bytes());
            out.extend_from_slice(value.as_bytes());
        }
    }
    Ok(out)
}
fn decode(bytes: &[u8], generation: u64, nonce: [u8; 16]) -> Option<Vec<QueuePreview>> {
    fn take<'a>(data: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
        let (a, b) = data.split_at_checked(n)?;
        *data = b;
        Some(a)
    }
    let mut data = bytes;
    if take(&mut data, 8)? != MAGIC
        || u64::from_le_bytes(take(&mut data, 8)?.try_into().ok()?) != generation
        || take(&mut data, 16)? != nonce
    {
        return None;
    }
    let count = take(&mut data, 1)?[0] as usize;
    if count > MAX_ROWS {
        return None;
    }
    let mut rows = Vec::with_capacity(count);
    let mut ids = std::collections::HashSet::new();
    for _ in 0..count {
        let id: [u8; 16] = take(&mut data, 16)?.try_into().ok()?;
        if id == [0; 16] || !ids.insert(id) {
            return None;
        }
        let action = match take(&mut data, 1)?[0] {
            0 => QueueAction::Copy,
            1 => QueueAction::Move,
            2 => QueueAction::Duplicate,
            _ => return None,
        };
        let flags = take(&mut data, 1)?[0];
        if flags & !3 != 0 {
            return None;
        }
        let item_count = u64::from_le_bytes(take(&mut data, 8)?.try_into().ok()?);
        let mut text = || -> Option<String> {
            let len = u16::from_le_bytes(take(&mut data, 2)?.try_into().ok()?) as usize;
            if len > MAX_TEXT {
                return None;
            }
            Some(std::str::from_utf8(take(&mut data, len)?).ok()?.to_owned())
        };
        let source = text()?;
        let destination = text()?;
        rows.push(QueuePreview {
            receipt_id: TransferReceiptId(id),
            action,
            source,
            destination,
            item_count,
            verify_requested: flags & 1 != 0,
            truncated: flags & 2 != 0,
        });
    }
    if !data.is_empty() {
        return None;
    }
    Some(rows)
}
struct Slot {
    leaf: OsString,
    generation: u64,
    identity: NativeFileIdentity,
    bytes: u64,
    rows: Option<Vec<QueuePreview>>,
}
fn slots(location: &ReceiptLocation) -> Result<Vec<Slot>, ReceiptStoreError> {
    let cancel = ScanCancellation::new();
    let held = HeldParent::open_with_access(&location.path, &cancel, DirectoryAccess::List)
        .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
    if held.snapshot != location.held.snapshot {
        return Err(ReceiptStoreError::UnsafeNamespace);
    }
    let entries = enumerate_transfer_directory(
        held.directory().file(),
        &location.path,
        MAX_CATALOG_ENTRIES,
        MAX_CATALOG_ENTRIES * 512,
        || false,
    )
    .map_err(|_| ReceiptStoreError::IncompleteScan)?;
    let mut slots = Vec::new();
    let mut total = 0u64;
    for entry in entries {
        let Some((generation, nonce)) = parse_slot_name(&entry.name) else {
            continue;
        };
        if !matches!(entry.kind, TransferDirectoryEntryKind::Ordinary) {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        let mut file = held
            .directory()
            .open_receipt_read(&entry.name)
            .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        let bytes = file.file().metadata()?.len();
        total = total
            .checked_add(bytes)
            .ok_or(ReceiptStoreError::Capacity)?;
        if total > QUEUE_RESERVE {
            return Err(ReceiptStoreError::Capacity);
        }
        let rows = if bytes <= QUEUE_RESERVE {
            match read_prefix_from_file(file.file_mut(), QUEUE_RESERVE) {
                Ok(frames)
                    if frames.problem == FrameProblem::Clean && frames.records.len() == 1 =>
                {
                    decode(&frames.records[0], generation, nonce)
                }
                _ => None,
            }
        } else {
            None
        };
        file.validate()
            .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        if file.file().metadata()?.len() != bytes {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        slots.push(Slot {
            leaf: entry.name,
            generation,
            identity: file.identity(),
            bytes,
            rows,
        });
    }
    held.check_chain(&cancel)
        .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
    let mut generations = std::collections::HashSet::new();
    if slots
        .iter()
        .any(|slot| !generations.insert(slot.generation))
    {
        return Err(ReceiptStoreError::UnsafeNamespace);
    }
    Ok(slots)
}
/// Count known physical files without opening their contents. Active applied
/// writers deny data-read sharing but allow metadata-only relative opens.
fn physical_usage(location: &ReceiptLocation) -> Result<(u64, usize), ReceiptStoreError> {
    let cancel = ScanCancellation::new();
    let held = HeldParent::open_with_access(&location.path, &cancel, DirectoryAccess::List)
        .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
    if held.snapshot != location.held.snapshot {
        return Err(ReceiptStoreError::UnsafeNamespace);
    }
    let entries = enumerate_transfer_directory(
        held.directory().file(),
        &location.path,
        MAX_CATALOG_ENTRIES,
        MAX_CATALOG_ENTRIES * 512,
        || false,
    )
    .map_err(|_| ReceiptStoreError::IncompleteScan)?;
    let mut total = 0u64;
    for entry in &entries {
        if parse_name(&entry.name).is_none() && parse_slot_name(&entry.name).is_none() {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        if !matches!(entry.kind, TransferDirectoryEntryKind::Ordinary) {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        let file = held
            .directory()
            .open_relative_metadata(&entry.name)
            .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        let meta = file.metadata()?;
        let identity = crate::file_operations::native_file_identity(&file)?;
        let actual = crate::file_operations::final_path_for_handle(&file)?;
        let parent_path = crate::file_operations::final_path_for_handle(held.directory().file())?;
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            FILE_STANDARD_INFO, FileStandardInfo, GetFileInformationByHandleEx,
        };
        let mut standard = FILE_STANDARD_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileStandardInfo,
                std::ptr::from_mut(&mut standard).cast(),
                std::mem::size_of::<FILE_STANDARD_INFO>() as u32,
            )
        }
        .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        if !meta.is_file()
            || standard.Directory.0 != 0
            || standard.DeletePending.0 != 0
            || standard.NumberOfLinks != 1
            || !crate::batch_rename::compare_paths_windows_ordinal(
                &parent_path.join(&entry.name),
                &actual,
            )
            .is_eq()
            || crate::file_operations::has_cloud_placeholder_attributes(meta.file_attributes())
            || meta.file_attributes()
                & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                != 0
            || identity.file_id() == [0; 16]
            || identity.volume_serial_number() != held.snapshot.identity().volume_serial_number()
            || actual.file_name() != Some(entry.name.as_os_str())
        {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        total = total
            .checked_add(meta.len())
            .ok_or(ReceiptStoreError::Capacity)?;
    }
    held.check_chain(&cancel)
        .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
    Ok((total, entries.len()))
}

impl ReceiptStore {
    pub(in crate::file_transfer) fn load_queue_snapshot(
        &self,
    ) -> Result<QueueSnapshotRead, ReceiptStoreError> {
        let _lock = self.queue_lock.lock().unwrap_or_else(|e| e.into_inner());
        let Some(location) = self.locate(false)? else {
            return Ok(QueueSnapshotRead::default());
        };
        let mut slots = slots(&location)?;
        slots.sort_by_key(|s| std::cmp::Reverse(s.generation));
        let mut result = QueueSnapshotRead::default();
        for slot in slots {
            if let Some(rows) = slot.rows {
                result.rows = rows;
                break;
            }
            result.newer_incomplete = true;
        }
        Ok(result)
    }
    pub(in crate::file_transfer) fn save_queue_snapshot(
        &self,
        rows: &[QueuePreview],
    ) -> Result<(), ReceiptStoreError> {
        let _lock = self.queue_lock.lock().unwrap_or_else(|e| e.into_inner());
        encode(1, [1; 16], rows)?;
        let location = self.locate(true)?.ok_or(ReceiptStoreError::Unavailable)?;
        let mut old = slots(&location)?;
        let generation = old
            .iter()
            .map(|s| s.generation)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(ReceiptStoreError::Capacity)?;
        let mut rng = rand::rngs::SysRng;
        let mut nonce = [0; 16];
        nonce[..8].copy_from_slice(
            &rng.try_next_u64()
                .map_err(|_| ReceiptStoreError::Unavailable)?
                .to_le_bytes(),
        );
        nonce[8..].copy_from_slice(
            &rng.try_next_u64()
                .map_err(|_| ReceiptStoreError::Unavailable)?
                .to_le_bytes(),
        );
        if nonce == [0; 16] {
            return Err(ReceiptStoreError::Unavailable);
        }
        let payload = encode(generation, nonce, rows)?;
        let needed = FILE_HEADER
            .checked_add(FrameWriter::frame_size(payload.len()).ok_or(ReceiptStoreError::Capacity)?)
            .ok_or(ReceiptStoreError::Capacity)?;
        let used = old
            .iter()
            .try_fold(0u64, |sum, s| sum.checked_add(s.bytes))
            .ok_or(ReceiptStoreError::Capacity)?;
        if used.checked_add(needed).is_none_or(|n| n > QUEUE_RESERVE) {
            return Err(ReceiptStoreError::Capacity);
        }
        if old.len() >= QUEUE_FILE_RESERVE {
            return Err(ReceiptStoreError::Capacity);
        }
        let (charged_bytes, physical_count) = physical_usage(&location)?;
        if physical_count >= MAX_CATALOG_ENTRIES
            || charged_bytes
                .checked_add(needed)
                .is_none_or(|n| n > MAX_RECEIPT_BYTES)
        {
            return Err(ReceiptStoreError::Capacity);
        }
        let cancel = ScanCancellation::new();
        let held =
            HeldParent::open_with_access(&location.path, &cancel, DirectoryAccess::CreateFile)
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        if held.snapshot != location.held.snapshot {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        let leaf = slot_name(generation, nonce);
        let guard = held.directory().create_receipt(&leaf).map_err(|error| {
            if error.unconfirmed_name_effect() {
                ReceiptStoreError::QueueCommitUncertain
            } else {
                ReceiptStoreError::UnsafeNamespace
            }
        })?;
        // Once a candidate exists any write, sync or final guard failure may
        // leave a durable complete generation, so report an uncertain commit.
        let commit = (|| -> Result<(), ReceiptStoreError> {
            let mut writer = FrameWriter::from_new_file(guard.try_clone_file()?, needed)?;
            guard
                .validate()
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            writer.append(&payload)?;
            guard
                .validate()
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            drop(writer);
            Ok(())
        })();
        if commit.is_err() {
            return Err(ReceiptStoreError::QueueCommitUncertain);
        }
        drop(guard);
        // Only verified complete prior generations can be reclaimed. A torn or
        // unrecognized file is never treated as owned merely from its name.
        old.sort_by_key(|s| std::cmp::Reverse(s.generation));
        // Preserve the newest complete predecessor as the fallback.
        for slot in old.into_iter().filter(|s| s.rows.is_some()).skip(1) {
            let cleanup = (|| -> Result<(), ReceiptStoreError> {
                let mut file = held
                    .directory()
                    .open_receipt_prune(&slot.leaf)
                    .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
                if file.identity() != slot.identity || file.file().metadata()?.len() != slot.bytes {
                    return Err(ReceiptStoreError::UnsafeNamespace);
                }
                let frames = read_prefix_from_file(file.file_mut(), QUEUE_RESERVE)?;
                let (generation, nonce) =
                    parse_slot_name(&slot.leaf).ok_or(ReceiptStoreError::UnsafeNamespace)?;
                if frames.problem != FrameProblem::Clean
                    || frames.records.len() != 1
                    || decode(&frames.records[0], generation, nonce) != slot.rows
                {
                    return Err(ReceiptStoreError::UnsafeNamespace);
                }
                file.validate()
                    .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
                let actual = crate::file_operations::final_path_for_handle(file.file())?;
                if actual.file_name() != Some(slot.leaf.as_os_str()) {
                    return Err(ReceiptStoreError::UnsafeNamespace);
                }
                file.mark_delete_pending()
                    .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
                drop(file);
                let leaf = crate::file_transfer::TransferLeaf::new(&slot.leaf)
                    .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
                if !matches!(
                    held.inspect_leaf(&leaf, &cancel),
                    Ok(crate::file_transfer::TargetOccupancy::Vacant)
                ) {
                    return Err(ReceiptStoreError::UnsafeNamespace);
                }
                Ok(())
            })();
            // The new generation is already committed; retained older data is
            // conservative and must not turn success into a failed enqueue.
            let _ = cleanup;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn row(id: u8, action: QueueAction) -> QueuePreview {
        QueuePreview {
            receipt_id: TransferReceiptId([id; 16]),
            action,
            source: format!("source {id}"),
            destination: format!("destination {id}"),
            item_count: 3,
            verify_requested: true,
            truncated: false,
        }
    }
    #[test]
    fn receipt_queue_saves_during_live_receipt_session() {
        use crate::file_transfer::{
            TransferAction, TransferDraft, inspect_source_tree, review_transfer,
        };
        use std::sync::Arc;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("input.txt");
        let destination = temp.path().join("destination");
        std::fs::write(&source, b"data").unwrap();
        std::fs::create_dir(&destination).unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::CopyTo(destination),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut store = ReceiptStore::new(&AppDataRoot::from_path(temp.path()));
        let queue_store = store.clone();
        let session = store.admit(&plan).unwrap();
        let rows = vec![row(7, QueueAction::Copy)];
        queue_store.save_queue_snapshot(&rows).unwrap();
        assert_eq!(queue_store.load_queue_snapshot().unwrap().rows, rows);
        drop(session);
    }
    #[test]
    fn receipt_queue_committed_generation_survives_old_prune_refusal() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
        let temp = tempfile::tempdir().unwrap();
        let store = ReceiptStore::new(&AppDataRoot::from_path(temp.path()));
        store
            .save_queue_snapshot(&[row(1, QueueAction::Copy)])
            .unwrap();
        store
            .save_queue_snapshot(&[row(2, QueueAction::Move)])
            .unwrap();
        let location = store.locate(false).unwrap().unwrap();
        let old = slots(&location).unwrap();
        let oldest = old.iter().min_by_key(|slot| slot.generation).unwrap();
        let previous = old.iter().max_by_key(|slot| slot.generation).unwrap();
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(location.path.join(&oldest.leaf))
            .unwrap();
        let rows = vec![row(3, QueueAction::Duplicate)];
        store.save_queue_snapshot(&rows).unwrap();
        assert_eq!(store.load_queue_snapshot().unwrap().rows, rows);
        assert!(location.path.join(&oldest.leaf).exists());
        assert!(location.path.join(&previous.leaf).exists());
        drop(locked);
    }
    #[test]
    fn receipt_queue_order_and_shared_profile() {
        let temp = tempfile::tempdir().unwrap();
        let store = ReceiptStore::new(&AppDataRoot::from_path(temp.path()));
        assert_eq!(
            store.load_queue_snapshot().unwrap(),
            QueueSnapshotRead::default()
        );
        assert!(!temp.path().join("transfers").exists());
        let rows = vec![row(1, QueueAction::Copy), row(2, QueueAction::Move)];
        store.save_queue_snapshot(&rows).unwrap();
        assert_eq!(store.clone().load_queue_snapshot().unwrap().rows, rows);
        let reversed = vec![rows[1].clone(), rows[0].clone()];
        store.save_queue_snapshot(&reversed).unwrap();
        assert_eq!(store.load_queue_snapshot().unwrap().rows, reversed);
    }
    #[test]
    fn receipt_queue_torn_newer_is_preserved_and_prior_remains_readable() {
        let temp = tempfile::tempdir().unwrap();
        let store = ReceiptStore::new(&AppDataRoot::from_path(temp.path()));
        let rows = vec![row(4, QueueAction::Duplicate)];
        store.save_queue_snapshot(&rows).unwrap();
        let location = store.locate(true).unwrap().unwrap();
        let cancel = ScanCancellation::new();
        let held =
            HeldParent::open_with_access(&location.path, &cancel, DirectoryAccess::CreateFile)
                .unwrap();
        let name = slot_name(2, [3; 16]);
        let guard = held.directory().create_receipt(&name).unwrap();
        let mut file = guard.try_clone_file().unwrap();
        file.write_all(b"torn").unwrap();
        file.sync_all().unwrap();
        drop(file);
        drop(guard);
        let read = store.load_queue_snapshot().unwrap();
        assert_eq!(read.rows, rows);
        assert!(read.newer_incomplete);
        store
            .save_queue_snapshot(&[row(5, QueueAction::Move)])
            .unwrap();
        assert!(location.path.join(name).exists());
    }
    #[test]
    fn receipt_queue_rejects_oversize_without_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let store = ReceiptStore::new(&AppDataRoot::from_path(temp.path()));
        let mut bad = row(1, QueueAction::Copy);
        bad.source = "x".repeat(MAX_TEXT + 1);
        assert!(store.save_queue_snapshot(&[bad]).is_err());
        assert!(!temp.path().join("transfers").exists());
    }
}
