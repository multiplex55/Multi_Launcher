//! Bounded durable receipt catalogue. Reading never creates the namespace.
use super::adapter::ReceiptProjection;
use super::frames::{
    FrameProblem, FrameWriteError, FrameWriter, MAX_RECEIPT_BYTES, read_prefix_from_file,
};
use super::wire::{
    self, Disposition, EntryOutcome, JobOutcome, NodeId, ReceiptId, Record, SemanticPrefix,
    SemanticProblem,
};
use crate::archive::transfer_native::{ReceiptFile, is_missing_relative_leaf};
use crate::file_operations::{
    NativeFileIdentity, ScanCancellation, TransferDirectoryEntryKind, enumerate_transfer_directory,
};
use crate::file_transfer::journal::TransferJournalEvent;
use crate::file_transfer::{
    ApprovedTransferPlan, DirectoryAccess, HeldParent, SourceNodeId, TargetSnapshot,
};
use crate::platform::app_data::AppDataRoot;
use rand::TryRng;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

const LIVE_RESERVE: u64 = 33 * 1024;
const ENTRY_RESERVE: u64 = 130;
const JOB_RESERVE: u64 = 80;
const FILE_HEADER: u64 = 12;
const MAX_CATALOG_ENTRIES: usize = 10_000;

#[derive(Debug)]
pub(in crate::file_transfer) enum ReceiptStoreError {
    Unavailable,
    UnsafeNamespace,
    IncompleteScan,
    Capacity,
    Collision,
    InvalidRecord,
    Poisoned,
    Io(io::ErrorKind),
    AfterCreate {
        id: [u8; 16],
        reason: Box<ReceiptStoreError>,
    },
}
impl ReceiptStoreError {
    pub fn receipt_id(&self) -> Option<[u8; 16]> {
        match self {
            Self::AfterCreate { id, .. } => Some(*id),
            _ => None,
        }
    }
}
impl From<io::Error> for ReceiptStoreError {
    fn from(value: io::Error) -> Self {
        Self::Io(value.kind())
    }
}
impl From<FrameWriteError> for ReceiptStoreError {
    fn from(value: FrameWriteError) -> Self {
        match value {
            FrameWriteError::Io(error) => Self::Io(error.kind()),
            FrameWriteError::Poisoned => Self::Poisoned,
            FrameWriteError::Capacity | FrameWriteError::FrameTooLarge => Self::Capacity,
            _ => Self::InvalidRecord,
        }
    }
}
impl From<SemanticProblem> for ReceiptStoreError {
    fn from(_: SemanticProblem) -> Self {
        Self::InvalidRecord
    }
}

#[derive(Debug)]
pub(super) enum ReceiptDiagnostic {
    Clean,
    Frame(FrameProblem),
    Semantic(SemanticProblem),
    NameMismatch,
    UnknownName,
    Oversized,
    DecodeBudget,
}
pub(super) struct CatalogEntry {
    pub leaf: OsString,
    pub id: Option<ReceiptId>,
    pub identity: NativeFileIdentity,
    pub bytes: u64,
    pub created: u64,
    pub prefix: SemanticPrefix,
    pub diagnostic: ReceiptDiagnostic,
    pub settled: bool,
}
pub(super) struct ReceiptCatalog {
    pub absent: bool,
    pub entries: Vec<CatalogEntry>,
    pub charged_bytes: u64,
    pub admission_blocked: bool,
    pub unavailable: Vec<OsString>,
}
pub(in crate::file_transfer) struct ReceiptStore {
    root: PathBuf,
    root_proof: std::sync::OnceLock<crate::file_transfer::ParentSnapshot>,
}
struct ReceiptLocation {
    path: PathBuf,
    held: HeldParent,
}

impl ReceiptStore {
    pub fn new(root: &AppDataRoot) -> Self {
        Self {
            root: root.path().to_path_buf(),
            root_proof: std::sync::OnceLock::new(),
        }
    }
    pub(super) fn scan(&self) -> Result<ReceiptCatalog, ReceiptStoreError> {
        let Some(location) = self.locate(false)? else {
            return Ok(ReceiptCatalog {
                absent: true,
                entries: Vec::new(),
                charged_bytes: 0,
                admission_blocked: false,
                unavailable: Vec::new(),
            });
        };
        self.scan_location(&location)
    }
    fn scan_location(
        &self,
        location: &ReceiptLocation,
    ) -> Result<ReceiptCatalog, ReceiptStoreError> {
        let path = &location.path;
        let cancel = ScanCancellation::new();
        let held = HeldParent::open_with_access(path, &cancel, DirectoryAccess::List)
            .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        if held.snapshot != location.held.snapshot {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        let names = enumerate_transfer_directory(
            held.directory().file(),
            &path,
            MAX_CATALOG_ENTRIES,
            MAX_CATALOG_ENTRIES * 512,
            || false,
        )
        .map_err(|_| ReceiptStoreError::IncompleteScan)?;
        let mut catalog = ReceiptCatalog {
            absent: false,
            entries: Vec::new(),
            charged_bytes: 0,
            admission_blocked: false,
            unavailable: Vec::new(),
        };
        let mut decoded_bytes = 0u64;
        for entry in names {
            if !matches!(entry.kind, TransferDirectoryEntryKind::Ordinary) {
                catalog.admission_blocked = true;
                catalog.unavailable.push(entry.name);
                continue;
            }
            let mut file = match held.directory().open_receipt_read(&entry.name) {
                Ok(file) => file,
                Err(_) => {
                    held.check_chain(&cancel)
                        .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
                    catalog.admission_blocked = true;
                    catalog.unavailable.push(entry.name);
                    continue;
                }
            };
            if file.validate().is_err() {
                held.check_chain(&cancel)
                    .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
                catalog.admission_blocked = true;
                catalog.unavailable.push(entry.name);
                continue;
            }
            let metadata = match file.file().metadata() {
                Ok(metadata) => metadata,
                Err(_) => {
                    catalog.admission_blocked = true;
                    catalog.unavailable.push(entry.name);
                    continue;
                }
            };
            let bytes = metadata.len();
            catalog.charged_bytes = catalog
                .charged_bytes
                .checked_add(bytes)
                .ok_or(ReceiptStoreError::Capacity)?;
            let (prefix, mut diagnostic) = if bytes > MAX_RECEIPT_BYTES {
                (SemanticPrefix::default(), ReceiptDiagnostic::Oversized)
            } else if decoded_bytes
                .checked_add(bytes)
                .is_none_or(|n| n > MAX_RECEIPT_BYTES)
            {
                (SemanticPrefix::default(), ReceiptDiagnostic::DecodeBudget)
            } else {
                decoded_bytes += bytes;
                match parse(&mut file) {
                    Ok(parsed) => parsed,
                    Err(_) => {
                        held.check_chain(&cancel)
                            .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
                        catalog.admission_blocked = true;
                        catalog.unavailable.push(entry.name);
                        continue;
                    }
                }
            };
            if file.validate().is_err()
                || file.file().metadata().map(|m| m.len()).ok() != Some(bytes)
            {
                held.check_chain(&cancel)
                    .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
                catalog.admission_blocked = true;
                catalog.unavailable.push(entry.name);
                continue;
            }
            let expected = parse_name(&entry.name);
            let actual = match prefix.records.first() {
                Some(Record::Header { id, .. }) => Some(*id),
                _ => None,
            };
            if expected.is_none() {
                diagnostic = ReceiptDiagnostic::UnknownName;
                catalog.admission_blocked = true;
            } else if actual.is_some() && expected != actual {
                diagnostic = ReceiptDiagnostic::NameMismatch;
                catalog.admission_blocked = true;
            }
            let settled = matches!(diagnostic, ReceiptDiagnostic::Clean) && prefix.settled();
            catalog.entries.push(CatalogEntry {
                leaf: entry.name,
                id: actual,
                identity: file.identity(),
                bytes,
                created: metadata.creation_time(),
                prefix,
                diagnostic,
                settled,
            });
        }
        held.check_chain(&cancel)
            .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        location
            .held
            .check_chain(&cancel)
            .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        Ok(catalog)
    }
    pub(super) fn read_selected(
        &self,
        id: ReceiptId,
    ) -> Result<Option<CatalogEntry>, ReceiptStoreError> {
        let Some(location) = self.locate(false)? else {
            return Ok(None);
        };
        let name = receipt_name(id);
        let mut file = match location.held.directory().open_receipt_read(&name) {
            Ok(file) => file,
            Err(error) if is_missing_relative_leaf(&error) => return Ok(None),
            Err(_) => return Err(ReceiptStoreError::UnsafeNamespace),
        };
        file.validate()
            .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        let actual_path = crate::file_operations::final_path_for_handle(file.file())?;
        if actual_path.file_name() != Some(name.as_os_str()) {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        let metadata = file.file().metadata()?;
        let bytes = metadata.len();
        let (prefix, mut diagnostic) = if bytes > MAX_RECEIPT_BYTES {
            (SemanticPrefix::default(), ReceiptDiagnostic::Oversized)
        } else {
            parse(&mut file)?
        };
        file.validate()
            .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        if file.file().metadata()?.len() != bytes {
            return Err(ReceiptStoreError::IncompleteScan);
        }
        location
            .held
            .check_chain(&ScanCancellation::new())
            .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        let actual = match prefix.records.first() {
            Some(Record::Header { id, .. }) => Some(*id),
            _ => None,
        };
        if actual.is_some() && actual != Some(id) {
            diagnostic = ReceiptDiagnostic::NameMismatch;
        }
        let settled = matches!(diagnostic, ReceiptDiagnostic::Clean) && prefix.settled();
        Ok(Some(CatalogEntry {
            leaf: name,
            id: actual,
            identity: file.identity(),
            bytes,
            created: metadata.creation_time(),
            prefix,
            diagnostic,
            settled,
        }))
    }
    fn locate(&self, create: bool) -> Result<Option<ReceiptLocation>, ReceiptStoreError> {
        let mut path = match std::fs::canonicalize(&self.root) {
            Ok(root) => root,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if self.root_proof.get().is_some() {
                    return Err(ReceiptStoreError::UnsafeNamespace);
                }
                if !create {
                    return Ok(None);
                }
                return Err(error.into());
            }
            Err(error) => return Err(error.into()),
        };
        let cancel = ScanCancellation::new();
        let mut held = HeldParent::open_with_access(
            &path,
            &cancel,
            if create {
                DirectoryAccess::CreateDirectory
            } else {
                DirectoryAccess::Observe
            },
        )
        .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        let _ = self.root_proof.set(held.snapshot.clone());
        if self.root_proof.get() != Some(&held.snapshot) {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        for (index, leaf) in ["transfers", "receipts"].into_iter().enumerate() {
            let component = OsStr::new(leaf);
            let child = match held.directory().open_child(component) {
                Ok(child) => child,
                Err(error) if is_missing_relative_leaf(&error) && !create => return Ok(None),
                Err(error) if is_missing_relative_leaf(&error) && create => {
                    held.check_chain(&cancel)
                        .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
                    held.directory()
                        .create_directory(component)
                        .map_err(|_| ReceiptStoreError::UnsafeNamespace)?
                }
                Err(_) => return Err(ReceiptStoreError::UnsafeNamespace),
            };
            let identity = child
                .verify_identity()
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            path.push(leaf);
            let access = if create && index == 0 {
                DirectoryAccess::CreateDirectory
            } else {
                DirectoryAccess::Observe
            };
            let next = HeldParent::open_with_access(&path, &cancel, access)
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            if next.snapshot.identity() != identity
                || next.snapshot.chain.get(..held.snapshot.chain.len())
                    != Some(held.snapshot.chain.as_ref())
            {
                return Err(ReceiptStoreError::UnsafeNamespace);
            }
            held.check_chain(&cancel)
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            held = next;
        }
        Ok(Some(ReceiptLocation { path, held }))
    }
    pub fn admit<'a>(
        &'a mut self,
        plan: &ApprovedTransferPlan,
    ) -> Result<ReceiptSession<'a>, ReceiptStoreError> {
        let mut random = rand::rngs::SysRng;
        let mut id = [0u8; 16];
        id[..8].copy_from_slice(
            &random
                .try_next_u64()
                .map_err(|_| ReceiptStoreError::Unavailable)?
                .to_le_bytes(),
        );
        id[8..].copy_from_slice(
            &random
                .try_next_u64()
                .map_err(|_| ReceiptStoreError::Unavailable)?
                .to_le_bytes(),
        );
        if id == [0; 16] {
            return Err(ReceiptStoreError::Unavailable);
        }
        let id = ReceiptId(id);
        let mut projection = ReceiptProjection::new(id, plan)?;
        let outline = projection.take_records();
        let mut encoded = Vec::with_capacity(outline.len());
        let mut initial = FILE_HEADER;
        for record in outline {
            let bytes = wire::encode(&record)?;
            initial = initial
                .checked_add(
                    FrameWriter::frame_size(bytes.len()).ok_or(ReceiptStoreError::Capacity)?,
                )
                .ok_or(ReceiptStoreError::Capacity)?;
            if initial > MAX_RECEIPT_BYTES {
                return Err(ReceiptStoreError::Capacity);
            }
            encoded.push(bytes);
        }
        let rows = u64::try_from(plan.source().entries().len())
            .map_err(|_| ReceiptStoreError::Capacity)?;
        let closing = rows
            .checked_mul(ENTRY_RESERVE)
            .and_then(|n| n.checked_add(JOB_RESERVE))
            .ok_or(ReceiptStoreError::Capacity)?;
        let needed = initial
            .checked_add(LIVE_RESERVE)
            .and_then(|n| n.checked_add(closing))
            .ok_or(ReceiptStoreError::Capacity)?;
        if needed > MAX_RECEIPT_BYTES {
            return Err(ReceiptStoreError::Capacity);
        }
        let location = self.locate(true)?.ok_or(ReceiptStoreError::Unavailable)?;
        let mut catalog = self.scan_location(&location)?;
        if catalog.admission_blocked {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        self.ensure_room(&location, &mut catalog, needed, MAX_CATALOG_ENTRIES)?;
        let capacity = MAX_RECEIPT_BYTES
            .checked_sub(catalog.charged_bytes)
            .ok_or(ReceiptStoreError::Capacity)?;
        if capacity < needed {
            return Err(ReceiptStoreError::Capacity);
        }
        let cancel = ScanCancellation::new();
        let held =
            HeldParent::open_with_access(&location.path, &cancel, DirectoryAccess::CreateFile)
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        if held.snapshot != location.held.snapshot {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        let leaf = receipt_name(id);
        let guard = held.directory().create_receipt(&leaf).map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                ReceiptStoreError::Collision
            } else {
                ReceiptStoreError::AfterCreate {
                    id: id.0,
                    reason: Box::new(ReceiptStoreError::UnsafeNamespace),
                }
            }
        })?;
        let writer = (|| -> Result<FrameWriter, ReceiptStoreError> {
            guard
                .validate()
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            let mut writer = FrameWriter::from_new_file(guard.try_clone_file()?, capacity)?;
            guard
                .validate()
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            for bytes in encoded {
                guard
                    .validate()
                    .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
                writer.append(&bytes)?;
                guard
                    .validate()
                    .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            }
            Ok(writer)
        })()
        .map_err(|reason| ReceiptStoreError::AfterCreate {
            id: id.0,
            reason: Box::new(reason),
        })?;
        Ok(ReceiptSession {
            _store: self,
            id,
            _held: held,
            guard,
            writer,
            projection,
            closing,
            rows_remaining: rows,
            poisoned: false,
            in_operation: false,
            operation_remaining: 0,
        })
    }
    fn ensure_room(
        &self,
        location: &ReceiptLocation,
        catalog: &mut ReceiptCatalog,
        needed: u64,
        entry_limit: usize,
    ) -> Result<(), ReceiptStoreError> {
        if entry_limit == 0 {
            return Err(ReceiptStoreError::Capacity);
        }
        let unprunable_count = catalog
            .entries
            .iter()
            .filter(|entry| !entry.settled)
            .count();
        let unprunable_bytes = catalog
            .entries
            .iter()
            .filter(|entry| !entry.settled)
            .try_fold(0u64, |sum, entry| sum.checked_add(entry.bytes))
            .ok_or(ReceiptStoreError::Capacity)?;
        if unprunable_count >= entry_limit
            || unprunable_bytes
                .checked_add(needed)
                .is_none_or(|sum| sum > MAX_RECEIPT_BYTES)
        {
            return Err(ReceiptStoreError::Capacity);
        }
        if catalog.entries.len() >= entry_limit
            || catalog
                .charged_bytes
                .checked_add(needed)
                .is_none_or(|sum| sum > MAX_RECEIPT_BYTES)
        {
            self.prune(location, catalog, needed, entry_limit)?;
        }
        if catalog.entries.len() >= entry_limit
            || catalog
                .charged_bytes
                .checked_add(needed)
                .is_none_or(|sum| sum > MAX_RECEIPT_BYTES)
        {
            return Err(ReceiptStoreError::Capacity);
        }
        Ok(())
    }
    fn prune(
        &self,
        location: &ReceiptLocation,
        catalog: &mut ReceiptCatalog,
        needed: u64,
        entry_limit: usize,
    ) -> Result<(), ReceiptStoreError> {
        let cancel = ScanCancellation::new();
        let held = HeldParent::open(&location.path, &cancel)
            .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
        if held.snapshot != location.held.snapshot {
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        catalog.entries.sort_by_key(|entry| entry.created);
        let mut remaining = catalog.entries.len();
        let mut removed = std::collections::HashSet::new();
        for entry in &catalog.entries {
            if catalog
                .charged_bytes
                .checked_add(needed)
                .ok_or(ReceiptStoreError::Capacity)?
                <= MAX_RECEIPT_BYTES
                && remaining < entry_limit
            {
                break;
            }
            if !entry.settled {
                continue;
            }
            let mut guard = held
                .directory()
                .open_receipt_prune(&entry.leaf)
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            guard
                .validate()
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            if guard.identity() != entry.identity || guard.file().metadata()?.len() != entry.bytes {
                return Err(ReceiptStoreError::UnsafeNamespace);
            }
            let (prefix, diagnostic) = parse(&mut guard)?;
            let id = match prefix.records.first() {
                Some(Record::Header { id, .. }) => Some(*id),
                _ => None,
            };
            if !matches!(diagnostic, ReceiptDiagnostic::Clean)
                || !prefix.settled()
                || id != entry.id
                || id != parse_name(&entry.leaf)
            {
                return Err(ReceiptStoreError::UnsafeNamespace);
            }
            guard
                .validate()
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            if guard.file().metadata()?.len() != entry.bytes {
                return Err(ReceiptStoreError::UnsafeNamespace);
            }
            guard
                .mark_delete_pending()
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            drop(guard);
            let leaf = crate::file_transfer::TransferLeaf::new(&entry.leaf)
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?;
            match held
                .inspect_leaf(&leaf, &cancel)
                .map_err(|_| ReceiptStoreError::UnsafeNamespace)?
            {
                crate::file_transfer::TargetOccupancy::Vacant => {}
                crate::file_transfer::TargetOccupancy::Occupied(other)
                    if other.identity() != entry.identity =>
                {
                    return Err(ReceiptStoreError::UnsafeNamespace);
                }
                _ => return Err(ReceiptStoreError::UnsafeNamespace),
            }
            catalog.charged_bytes = catalog
                .charged_bytes
                .checked_sub(entry.bytes)
                .ok_or(ReceiptStoreError::UnsafeNamespace)?;
            remaining -= 1;
            removed.insert(entry.leaf.clone());
        }
        catalog
            .entries
            .retain(|entry| !removed.contains(&entry.leaf));
        Ok(())
    }
}

fn parse(file: &mut ReceiptFile) -> Result<(SemanticPrefix, ReceiptDiagnostic), ReceiptStoreError> {
    let frames = read_prefix_from_file(file.file_mut(), MAX_RECEIPT_BYTES)?;
    let mut prefix = SemanticPrefix::default();
    for payload in &frames.records {
        if !prefix.push_bytes(payload) {
            break;
        }
    }
    let diagnostic = if let Some(problem) = prefix.problem {
        ReceiptDiagnostic::Semantic(problem)
    } else if frames.problem != FrameProblem::Clean {
        ReceiptDiagnostic::Frame(frames.problem)
    } else {
        ReceiptDiagnostic::Clean
    };
    Ok((prefix, diagnostic))
}
fn receipt_name(id: ReceiptId) -> OsString {
    use std::fmt::Write;
    let mut name = String::with_capacity(40);
    for byte in id.0 {
        let _ = write!(&mut name, "{byte:02x}");
    }
    name.push_str(".receipt");
    name.into()
}
pub(super) fn parse_name(name: &OsStr) -> Option<ReceiptId> {
    let name = name.to_str()?;
    let hex = name.strip_suffix(".receipt")?;
    if hex.len() != 32
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let mut id = [0; 16];
    for (i, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    if id == [0; 16] {
        return None;
    }
    Some(ReceiptId(id))
}

pub(in crate::file_transfer) struct ReceiptSession<'a> {
    _store: &'a mut ReceiptStore,
    id: ReceiptId,
    _held: HeldParent,
    guard: ReceiptFile,
    writer: FrameWriter,
    projection: ReceiptProjection,
    closing: u64,
    rows_remaining: u64,
    poisoned: bool,
    in_operation: bool,
    operation_remaining: u64,
}
impl ReceiptSession<'_> {
    #[cfg(test)]
    pub(in crate::file_transfer) fn poison_terminal_for_test(&mut self) {
        self.poisoned = true;
    }
    pub fn receipt_id(&self) -> [u8; 16] {
        self.id.0
    }
    fn ensure(&mut self) -> Result<(), ReceiptStoreError> {
        if self.poisoned {
            return Err(ReceiptStoreError::Poisoned);
        }
        if self.guard.validate().is_err() {
            self.poisoned = true;
            return Err(ReceiptStoreError::UnsafeNamespace);
        }
        Ok(())
    }
    pub fn suggested_disposition(&self, id: SourceNodeId) -> Option<Disposition> {
        self.projection
            .prefix
            .suggested_disposition(NodeId(u32::try_from(id.index()).ok()?))
    }
    pub fn begin_operation(&mut self) -> Result<(), ReceiptStoreError> {
        self.ensure()?;
        if self.in_operation {
            return Err(ReceiptStoreError::InvalidRecord);
        }
        if self.writer.remaining_bytes()
            < self
                .closing
                .checked_add(LIVE_RESERVE)
                .ok_or(ReceiptStoreError::Capacity)?
        {
            return Err(ReceiptStoreError::Capacity);
        }
        self.in_operation = true;
        self.operation_remaining = LIVE_RESERVE;
        Ok(())
    }
    pub fn end_operation(&mut self) {
        self.in_operation = false;
    }
    fn flush_projection(&mut self, terminal: bool) -> Result<(), ReceiptStoreError> {
        let records = self.projection.take_records();
        for record in records {
            let bytes = wire::encode(&record)?;
            let length = FrameWriter::frame_size(bytes.len()).ok_or(ReceiptStoreError::Capacity)?;
            if !terminal {
                if !self.in_operation || length > self.operation_remaining {
                    self.poisoned = true;
                    return Err(ReceiptStoreError::Capacity);
                }
                self.operation_remaining -= length;
            }
            if self
                .writer
                .remaining_bytes()
                .checked_sub(length)
                .is_none_or(|n| n < self.closing)
            {
                self.poisoned = true;
                return Err(ReceiptStoreError::Capacity);
            }
            if let Err(error) = self
                .ensure()
                .and_then(|_| self.writer.append(&bytes).map_err(ReceiptStoreError::from))
                .and_then(|_| self.ensure())
            {
                self.poisoned = true;
                return Err(error);
            }
        }
        Ok(())
    }
    pub fn begin_entry(
        &mut self,
        id: SourceNodeId,
        target: &TargetSnapshot,
    ) -> Result<(), ReceiptStoreError> {
        if !self.in_operation {
            return Err(ReceiptStoreError::InvalidRecord);
        }
        if let Err(error) = self.projection.begin_entry(id, target) {
            self.poisoned = true;
            return Err(error.into());
        }
        self.flush_projection(false)
    }
    pub fn select_entry(&mut self, id: SourceNodeId) -> Result<(), ReceiptStoreError> {
        if !self.in_operation {
            return Err(ReceiptStoreError::InvalidRecord);
        }
        self.projection.select_entry(id).map_err(Into::into)
    }
    pub fn record_event(&mut self, event: &TransferJournalEvent) -> Result<(), ReceiptStoreError> {
        if !self.in_operation {
            return Err(ReceiptStoreError::InvalidRecord);
        }
        if let Err(error) = self.projection.project_event(event) {
            self.poisoned = true;
            return Err(error.into());
        }
        self.flush_projection(false)
    }
    pub fn finish_entry(
        &mut self,
        id: SourceNodeId,
        outcome: EntryOutcome,
        disposition: Disposition,
    ) -> Result<(), ReceiptStoreError> {
        if self.rows_remaining == 0 {
            return Err(ReceiptStoreError::InvalidRecord);
        }
        let record = Record::EntryTerminal {
            node: NodeId(u32::try_from(id.index()).map_err(|_| ReceiptStoreError::InvalidRecord)?),
            outcome,
            disposition,
        };
        let bytes = wire::encode(&record)?;
        if FrameWriter::frame_size(bytes.len()).is_none_or(|n| n > ENTRY_RESERVE) {
            return Err(ReceiptStoreError::Capacity);
        }
        self.projection.add(record)?;
        self.closing -= ENTRY_RESERVE;
        self.rows_remaining -= 1;
        self.flush_projection(true)
    }
    pub fn finish_job(&mut self, outcome: JobOutcome) -> Result<(), ReceiptStoreError> {
        if self.rows_remaining != 0 {
            return Err(ReceiptStoreError::InvalidRecord);
        }
        let record = Record::Finished { outcome };
        let bytes = wire::encode(&record)?;
        if FrameWriter::frame_size(bytes.len()).is_none_or(|n| n > JOB_RESERVE) {
            return Err(ReceiptStoreError::Capacity);
        }
        self.projection.add(record)?;
        self.closing -= JOB_RESERVE;
        self.flush_projection(true)
    }
}
impl crate::file_transfer::journal::TransferJournal for ReceiptSession<'_> {
    fn begin_operation(&mut self) -> io::Result<()> {
        ReceiptSession::begin_operation(self)
            .map_err(|_| io::Error::other("durable receipt unavailable"))
    }
    fn end_operation(&mut self) {
        ReceiptSession::end_operation(self);
    }
    fn begin_entry(&mut self, id: SourceNodeId, target: &TargetSnapshot) -> io::Result<()> {
        ReceiptSession::begin_entry(self, id, target)
            .map_err(|_| io::Error::other("durable receipt unavailable"))
    }
    fn select_entry(&mut self, id: SourceNodeId) -> io::Result<()> {
        ReceiptSession::select_entry(self, id)
            .map_err(|_| io::Error::other("durable receipt unavailable"))
    }
    fn record(&mut self, event: TransferJournalEvent) -> io::Result<()> {
        self.record_event(&event)
            .map_err(|_| io::Error::other("durable receipt unavailable"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn receipt_store_absent_scan_is_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let store = ReceiptStore::new(&root);
        let catalog = store.scan().unwrap();
        assert!(catalog.absent);
        assert!(!temp.path().join("transfers").exists());
    }

    #[test]
    fn receipt_store_corrupt_ordinary_file_is_charged_and_preserved() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let path = temp.path().join("transfers").join("receipts");
        fs::create_dir_all(&path).unwrap();
        let file = path.join(receipt_name(ReceiptId([1; 16])));
        fs::write(&file, b"broken").unwrap();
        let store = ReceiptStore::new(&root);
        let catalog = store.scan().unwrap();
        assert_eq!(catalog.charged_bytes, 6);
        assert_eq!(catalog.entries.len(), 1);
        assert!(!catalog.entries[0].settled);
        assert!(matches!(
            catalog.entries[0].diagnostic,
            ReceiptDiagnostic::Frame(_)
        ));
        assert_eq!(fs::read(file).unwrap(), b"broken");
    }

    #[test]
    fn receipt_store_real_plan_roundtrip_and_entry_cap_prune() {
        use crate::file_transfer::{
            TransferAction, TransferDraft, inspect_source_tree, review_transfer,
        };
        use std::sync::Arc;
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("input.txt");
        let destination = temp.path().join("destination");
        fs::write(&source, b"hello").unwrap();
        fs::create_dir(&destination).unwrap();
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
        let mut store = ReceiptStore::new(&root);
        let mut session = store.admit(&plan).unwrap();
        let id = session.id;
        session
            .finish_entry(
                plan.source().entries()[0].id(),
                EntryOutcome::Cancelled,
                Disposition::NoEffect,
            )
            .unwrap();
        session.finish_job(JobOutcome::Cancelled).unwrap();
        drop(session);
        let location = store.locate(false).unwrap().unwrap();
        let mut catalog = store.scan_location(&location).unwrap();
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.entries[0].id, Some(id));
        assert!(catalog.entries[0].settled);
        assert!(matches!(
            catalog.entries[0].prefix.records.last(),
            Some(Record::Finished {
                outcome: JobOutcome::Cancelled
            })
        ));
        // A one-entry cap exercises the production entry-slot path without
        // creating ten thousand files.
        store.ensure_room(&location, &mut catalog, 1, 1).unwrap();
        assert!(catalog.entries.is_empty());
        assert_eq!(catalog.charged_bytes, 0);
        assert!(!location.path.join(receipt_name(id)).exists());
    }

    #[test]
    fn receipt_store_existing_file_in_namespace_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        fs::write(temp.path().join("transfers"), b"occupied").unwrap();
        let store = ReceiptStore::new(&root);
        assert!(matches!(
            store.scan(),
            Err(ReceiptStoreError::UnsafeNamespace)
        ));
        assert_eq!(
            fs::read(temp.path().join("transfers")).unwrap(),
            b"occupied"
        );
    }

    #[test]
    fn receipt_store_oversized_receipt_is_charged_without_decode() {
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let path = temp.path().join("transfers").join("receipts");
        fs::create_dir_all(&path).unwrap();
        let file = fs::File::create(path.join(receipt_name(ReceiptId([2; 16])))).unwrap();
        file.set_len(MAX_RECEIPT_BYTES + 1).unwrap();
        drop(file);
        let catalog = ReceiptStore::new(&root).scan().unwrap();
        assert_eq!(catalog.charged_bytes, MAX_RECEIPT_BYTES + 1);
        assert!(matches!(
            catalog.entries[0].diagnostic,
            ReceiptDiagnostic::Oversized
        ));
        assert!(!catalog.entries[0].settled);
    }

    #[test]
    fn receipt_store_rejects_changed_physical_root() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("profile");
        fs::create_dir(&profile).unwrap();
        let store = ReceiptStore::new(&AppDataRoot::from_path(&profile));
        assert!(store.scan().unwrap().absent);
        fs::rename(&profile, temp.path().join("old-profile")).unwrap();
        assert!(matches!(
            store.scan(),
            Err(ReceiptStoreError::UnsafeNamespace)
        ));
        fs::create_dir(&profile).unwrap();
        assert!(matches!(
            store.scan(),
            Err(ReceiptStoreError::UnsafeNamespace)
        ));
    }
    #[test]
    fn receipt_applied_worker_unverified_copy_and_activity() {
        use crate::activity::{
            ActivityCenter, ActivityLifecycle, ActivityOutcome, ActivityRetirement,
        };
        use crate::file_transfer::{
            TransferAction, TransferDraft, TransferJobStatus, TransferJobs, inspect_source_tree,
            review_transfer,
        };
        use std::{
            sync::Arc,
            time::{Duration, Instant},
        };
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("one.txt");
        let target = temp.path().join("out");
        fs::write(&source, b"one").unwrap();
        fs::create_dir(&target).unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::CopyTo(target.clone()),
                verify: false,
            },
            &cancel,
        )
        .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let activity = ActivityCenter::new();
        let mut jobs = TransferJobs::with_data_root(activity.clone(), root.clone());
        let id = jobs.enqueue(plan).unwrap();
        jobs.start().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let info = loop {
            let state = jobs.snapshot();
            if let Some(info) = state.history.into_iter().find(|row| row.id == id) {
                break info;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(info.status, TransferJobStatus::Completed);
        assert_eq!(fs::read(target.join("one.txt")).unwrap(), b"one");
        let snapshot = activity.snapshot();
        let row = snapshot
            .rows
            .iter()
            .find(|row| Some(row.key) == info.activity)
            .unwrap();
        assert_eq!(
            row.lifecycle,
            ActivityLifecycle::Terminal(ActivityOutcome::Completed)
        );
        assert_eq!(row.retirement, ActivityRetirement::Confirmed);
        let catalog = ReceiptStore::new(&root).scan().unwrap();
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(info.receipt_id.unwrap().0, catalog.entries[0].id.unwrap().0);
        assert!(
            catalog.entries[0]
                .prefix
                .records
                .iter()
                .any(|record| matches!(
                    record,
                    Record::Effect {
                        effect: wire::Effect::Verified {
                            verification: wire::Verification::NotRequested,
                            ..
                        },
                        ..
                    }
                ))
        );
        assert!(catalog.entries[0].settled);
    }

    #[test]
    fn receipt_applied_nested_copy_records_directory_metadata() {
        use crate::activity::ActivityCenter;
        use crate::file_transfer::{
            TransferAction, TransferDraft, TransferJobStatus, TransferJobs, inspect_source_tree,
            review_transfer,
        };
        use std::{
            sync::Arc,
            time::{Duration, Instant},
        };
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("source");
        let target = temp.path().join("out");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(source.join("nested").join("data.txt"), b"data").unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::CopyTo(target.clone()),
                verify: false,
            },
            &cancel,
        )
        .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut jobs = TransferJobs::with_data_root(ActivityCenter::new(), root.clone());
        jobs.enqueue(plan).unwrap();
        jobs.start().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let info = loop {
            let state = jobs.snapshot();
            if let Some(info) = state.history.into_iter().next() {
                break info;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(info.status, TransferJobStatus::Completed);
        assert_eq!(
            fs::read(target.join("source").join("nested").join("data.txt")).unwrap(),
            b"data"
        );
        let catalog = ReceiptStore::new(&root).scan().unwrap();
        assert!(
            catalog.entries[0]
                .prefix
                .records
                .iter()
                .filter(|record| matches!(
                    record,
                    Record::Effect {
                        effect: wire::Effect::DirectoryMetadataObserved { .. },
                        ..
                    }
                ))
                .count()
                >= 2
        );
        assert!(catalog.entries[0].settled);
    }
    #[test]
    fn receipt_applied_storage_refusal_precedes_any_copy_effect() {
        use crate::activity::ActivityCenter;
        use crate::file_transfer::{
            TransferAction, TransferDraft, TransferJobStatus, TransferJobs, inspect_source_tree,
            review_transfer,
        };
        use std::{
            sync::Arc,
            time::{Duration, Instant},
        };
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("source.txt");
        let target = temp.path().join("out");
        fs::write(&source, b"original").unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(temp.path().join("transfers"), b"unsafe occupant").unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source.clone()], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::CopyTo(target.clone()),
                verify: false,
            },
            &cancel,
        )
        .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut jobs = TransferJobs::with_data_root(ActivityCenter::new(), root);
        jobs.enqueue(plan).unwrap();
        jobs.start().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let info = loop {
            let state = jobs.snapshot();
            if let Some(info) = state.history.into_iter().next() {
                break info;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(info.status, TransferJobStatus::Failed);
        assert!(!target.join("source.txt").exists());
        assert_eq!(fs::read(source).unwrap(), b"original");
    }
    #[test]
    fn receipt_applied_case_differing_directory_merge_removes_source() {
        use crate::activity::ActivityCenter;
        use crate::file_transfer::{
            ReviewChoice, TransferAction, TransferDraft, TransferJobStatus, TransferJobs,
            inspect_source_tree, review_transfer,
        };
        use std::{
            sync::Arc,
            time::{Duration, Instant},
        };
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("Source");
        let target = temp.path().join("out");
        fs::create_dir(&source).unwrap();
        fs::create_dir_all(target.join("source")).unwrap();
        fs::write(source.join("data.txt"), b"data").unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source.clone()], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::MoveTo(target.clone()),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let id = review.rows()[0].source();
        let review = review
            .choose(review.generation(), id, ReviewChoice::Merge, &cancel)
            .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut jobs = TransferJobs::with_data_root(ActivityCenter::new(), root.clone());
        jobs.enqueue(plan).unwrap();
        jobs.start().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let info = loop {
            let state = jobs.snapshot();
            if let Some(info) = state.history.into_iter().next() {
                break info;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(info.status, TransferJobStatus::Completed);
        assert!(!source.exists());
        assert_eq!(
            fs::read(target.join("source").join("data.txt")).unwrap(),
            b"data"
        );
        let catalog = ReceiptStore::new(&root).scan().unwrap();
        assert!(
            catalog.entries[0]
                .prefix
                .records
                .iter()
                .any(|record| matches!(
                    record,
                    Record::Effect {
                        effect: wire::Effect::DirectoryRemoveObserved {
                            observation: wire::Removal::Removed,
                            ..
                        },
                        ..
                    }
                ))
        );
    }
    #[test]
    fn receipt_applied_staged_replace_cancel_restores_source() {
        use crate::file_transfer::journal::TransferJournal;
        use crate::file_transfer::{
            CopyControl, CopyRecoveryArtifact, ExecutionState, ReviewChoice, StagedMoveSourceState,
            TransferAction, TransferDraft, inspect_source_tree, review_transfer,
        };
        use std::sync::Arc;
        struct CancelAfterDispose<'a, 'b> {
            receipt: &'a mut ReceiptSession<'b>,
            control: CopyControl,
        }
        impl TransferJournal for CancelAfterDispose<'_, '_> {
            fn begin_operation(&mut self) -> io::Result<()> {
                TransferJournal::begin_operation(self.receipt)
            }
            fn end_operation(&mut self) {
                TransferJournal::end_operation(self.receipt);
            }
            fn begin_entry(&mut self, id: SourceNodeId, target: &TargetSnapshot) -> io::Result<()> {
                TransferJournal::begin_entry(self.receipt, id, target)
            }
            fn select_entry(&mut self, id: SourceNodeId) -> io::Result<()> {
                TransferJournal::select_entry(self.receipt, id)
            }
            fn record(&mut self, event: TransferJournalEvent) -> io::Result<()> {
                let cancel = matches!(
                    &event,
                    TransferJournalEvent::StagedSourceIntent {
                        action: crate::file_transfer::copy::StagedSourceAction::Dispose,
                        ..
                    }
                );
                TransferJournal::record(self.receipt, event)?;
                if cancel {
                    self.control.cancel();
                }
                Ok(())
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let a = temp.path().join("from");
        let b = temp.path().join("to");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let source = a.join("same.txt");
        let target = b.join("same.txt");
        fs::write(&source, b"new content").unwrap();
        fs::write(&target, b"old content").unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source.clone()], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::MoveTo(b),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let review = review
            .choose(
                review.generation(),
                review.rows()[0].source(),
                ReviewChoice::Replace,
                &cancel,
            )
            .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut store = ReceiptStore::new(&root);
        let mut receipt = store.admit(&plan).unwrap();
        let control = CopyControl::default();
        let mut recorder = CancelAfterDispose {
            receipt: &mut receipt,
            control: control.clone(),
        };
        let report = crate::file_transfer::execute::execute_with_journal(
            &plan,
            &control,
            |_| {},
            &mut recorder,
        )
        .unwrap();
        drop(recorder);
        let ExecutionState::Cancelled(Some(error)) = &report.rows[0].state else {
            panic!("expected cancellation: {:?}", report.rows[0]);
        };
        let Some(CopyRecoveryArtifact::StagedMove(effect)) = &error.artifact else {
            panic!("missing staged effect")
        };
        assert_eq!(effect.source_state, StagedMoveSourceState::Original);
        assert_eq!(fs::read(&source).unwrap(), b"new content");
        assert_eq!(fs::read(&target).unwrap(), b"new content");
        let backup = error.replacement.as_ref().unwrap();
        assert_eq!(fs::read(&backup.backup).unwrap(), b"old content");
        receipt
            .finish_entry(
                report.rows[0].entry,
                EntryOutcome::Cancelled,
                Disposition::Retained,
            )
            .unwrap();
        receipt.finish_job(JobOutcome::Partial).unwrap();
        drop(receipt);
        let catalog = store.scan().unwrap();
        assert!(catalog.entries[0].prefix.records.iter().any(|r| matches!(
            r,
            Record::Effect {
                effect: wire::Effect::StagedSourceObserved {
                    action: wire::SourceAction::Restore,
                    state: wire::SourceState::Original,
                    ..
                },
                ..
            }
        )));
    }
    #[test]
    fn receipt_applied_verified_move_replace_retains_backup() {
        use crate::activity::ActivityCenter;
        use crate::file_transfer::{
            ReviewChoice, TransferAction, TransferDraft, TransferJobStatus, TransferJobs,
            inspect_source_tree, review_transfer,
        };
        use std::{
            sync::Arc,
            time::{Duration, Instant},
        };
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let a = temp.path().join("from");
        let b = temp.path().join("to");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let source = a.join("same.txt");
        let target = b.join("same.txt");
        fs::write(&source, b"new").unwrap();
        fs::write(&target, b"old").unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source.clone()], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::MoveTo(b),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let review = review
            .choose(
                review.generation(),
                review.rows()[0].source(),
                ReviewChoice::Replace,
                &cancel,
            )
            .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut jobs = TransferJobs::with_data_root(ActivityCenter::new(), root.clone());
        jobs.enqueue(plan).unwrap();
        jobs.start().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let info = loop {
            let state = jobs.snapshot();
            if let Some(info) = state.history.into_iter().next() {
                break info;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(info.status, TransferJobStatus::Completed);
        assert!(!source.exists());
        assert_eq!(fs::read(target).unwrap(), b"new");
        let catalog = ReceiptStore::new(&root).scan().unwrap();
        assert!(
            catalog.entries[0]
                .prefix
                .records
                .iter()
                .any(|record| matches!(
                    record,
                    Record::Effect {
                        effect: wire::Effect::Verified {
                            verification: wire::Verification::Verified { .. },
                            ..
                        },
                        ..
                    }
                ))
        );
        assert!(!catalog.entries[0].settled);
    }
    #[test]
    fn receipt_applied_terminal_refusal_preserves_report_and_partial_status() {
        use crate::activity::{
            ActivityCenter, ActivityDiagnostic, ActivityLifecycle, ActivityOutcome,
            ActivityRetirement,
        };
        use crate::file_transfer::{
            TransferAction, TransferDraft, TransferJobResult, TransferJobStatus, TransferJobs,
            inspect_source_tree, review_transfer,
        };
        use std::{
            sync::Arc,
            time::{Duration, Instant},
        };
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("data.txt");
        let target = temp.path().join("out");
        fs::write(&source, b"data").unwrap();
        fs::create_dir(&target).unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::CopyTo(target.clone()),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let activity = ActivityCenter::new();
        let mut jobs = TransferJobs::with_data_root(activity.clone(), root.clone());
        jobs.fail_terminal_for_test();
        let job = jobs.enqueue(plan).unwrap();
        jobs.start().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let info = loop {
            let state = jobs.snapshot();
            if let Some(info) = state.history.into_iter().find(|row| row.id == job) {
                break info;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(info.status, TransferJobStatus::Partial);
        assert_eq!(fs::read(target.join("data.txt")).unwrap(), b"data");
        let receipt_id = info.receipt_id.expect("durable id");
        assert!(
            matches!(info.result.as_deref(), Some(TransferJobResult::ReceiptFailed { report: Some(_), receipt_id: Some(found), .. }) if *found == receipt_id)
        );
        let snapshot = activity.snapshot();
        let row = snapshot
            .rows
            .iter()
            .find(|row| Some(row.key) == info.activity)
            .unwrap();
        assert_eq!(
            row.lifecycle,
            ActivityLifecycle::Terminal(ActivityOutcome::Partial)
        );
        assert_eq!(row.retirement, ActivityRetirement::Confirmed);
        assert_eq!(
            row.diagnostic,
            Some(ActivityDiagnostic::TransferReceiptUnavailable)
        );
        let catalog = ReceiptStore::new(&root).scan().unwrap();
        assert_eq!(catalog.entries[0].id.unwrap().0, receipt_id.0);
        assert!(!catalog.entries[0].settled);
        assert!(
            !catalog.entries[0]
                .prefix
                .records
                .iter()
                .any(|r| matches!(r, Record::Finished { .. }))
        );
    }
    #[test]
    fn receipt_applied_cancelled_directory_times_are_recorded_as_cancelled() {
        use crate::file_transfer::{
            CopyControl, DirectoryTimes, ExecutionProgress, ExecutionState, TransferAction,
            TransferDraft, TransferJobStatus, inspect_source_tree, review_transfer,
        };
        use std::sync::Arc;
        let temp = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(temp.path());
        let source = temp.path().join("dir");
        let target = temp.path().join("out");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(source.join("data"), b"data").unwrap();
        let cancel = ScanCancellation::new();
        let tree = Arc::new(inspect_source_tree(&[source], &cancel).unwrap());
        let review = review_transfer(
            tree,
            TransferDraft {
                action: TransferAction::CopyTo(target),
                verify: true,
            },
            &cancel,
        )
        .unwrap();
        let plan = review.approve(review.generation(), &cancel).unwrap();
        let mut store = ReceiptStore::new(&root);
        let mut receipt = store.admit(&plan).unwrap();
        let control = CopyControl::default();
        let report = crate::file_transfer::execute::execute_with_journal(
            &plan,
            &control,
            |event| {
                if matches!(event, ExecutionProgress::Finalizing(id) if id.index() == 0) {
                    control.cancel();
                }
            },
            &mut receipt,
        )
        .unwrap();
        assert!(
            matches!(&report.rows[0].state, ExecutionState::CreatedDirectory(directory) if matches!(directory.times, DirectoryTimes::Cancelled))
        );
        crate::file_transfer::jobs::finish_receipt(
            &mut receipt,
            &report,
            TransferJobStatus::Partial,
        )
        .unwrap();
        drop(receipt);
        let catalog = store.scan().unwrap();
        assert!(
            catalog.entries[0]
                .prefix
                .records
                .iter()
                .any(|record| matches!(
                    record,
                    Record::EntryTerminal {
                        node: NodeId(0),
                        outcome: EntryOutcome::Cancelled,
                        ..
                    }
                ))
        );
    }
}
