//! Bounded persistence for explicitly opened Calculation Pad documents.

use crate::calculation_pad::document::{MAX_DOCUMENT_BYTES, MAX_NAME_BYTES, MAX_SOURCE_LINES};
use crate::common::atomic_file::save_atomic;
use crate::platform::app_data::AppDataRoot;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const MAX_SAVED_DOCUMENTS: usize = 128;
pub const MAX_STORED_DOCUMENT_BYTES: usize = 512 * 1024;
pub const CURRENT_DOCUMENT_FORMAT_VERSION: u32 = 1;

const CALCULATION_PAD_DIRECTORY: &str = "calculation_pad";
const DOCUMENTS_DIRECTORY: &str = "documents";
const RECOVERY_FILE: &str = "last_source.json";
const MAX_DIRECTORY_ENTRIES: usize = MAX_SAVED_DOCUMENTS * 4;
pub const CURRENT_RECOVERY_FORMAT_VERSION: u32 = 1;
pub const MAX_STORED_RECOVERY_BYTES: usize = 512 * 1024;
const RECOVERY_WRITE_DEBOUNCE: Duration = Duration::from_millis(350);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadRecoverySnapshot {
    pub document_id: DocumentId,
    pub revision: u64,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadRecoveryWriteFailure {
    pub document_id: DocumentId,
    pub revision: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StoredRecovery {
    format_version: u32,
    document_id: DocumentId,
    revision: u64,
    source: String,
}

/// A single bounded source-only recovery record, separate from saved pads.
#[derive(Clone)]
pub struct PadRecoveryStore {
    root: PathBuf,
    pad_directory: PathBuf,
    path: PathBuf,
    transaction: Arc<Mutex<()>>,
    #[cfg(test)]
    test_before_write: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl PadRecoveryStore {
    pub fn new(root: &AppDataRoot) -> Self {
        let root = root.path().to_path_buf();
        let pad_directory = root.join(CALCULATION_PAD_DIRECTORY);
        let path = pad_directory.join(RECOVERY_FILE);
        let transaction = path_transaction(&pad_directory);
        Self {
            root,
            pad_directory,
            path,
            transaction,
            #[cfg(test)]
            test_before_write: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> anyhow::Result<Option<PadRecoverySnapshot>> {
        let _transaction = self.lock_transaction();
        if !self.ensure_existing_directory()? {
            return Ok(None);
        }
        let Some(bytes) = read_recovery_limited(&self.path)? else {
            return Ok(None);
        };
        let stored: StoredRecovery = serde_json::from_slice(&bytes)
            .map_err(|error| anyhow::anyhow!("invalid Calculation Pad recovery JSON: {error}"))?;
        if stored.format_version != CURRENT_RECOVERY_FORMAT_VERSION {
            anyhow::bail!(
                "unsupported Calculation Pad recovery version {} (supported version is {})",
                stored.format_version,
                CURRENT_RECOVERY_FORMAT_VERSION
            );
        }
        if stored.document_id.0 == 0 {
            anyhow::bail!("Calculation Pad recovery document identity is invalid");
        }
        validate_recovery_source(&stored.source)?;
        Ok(Some(PadRecoverySnapshot {
            document_id: stored.document_id,
            revision: stored.revision,
            source: stored.source,
        }))
    }

    fn save(&self, snapshot: &PadRecoverySnapshot) -> anyhow::Result<()> {
        #[cfg(test)]
        if let Some(hook) = &self.test_before_write {
            hook();
        }
        validate_recovery_source(&snapshot.source)?;
        if snapshot.document_id.0 == 0 {
            anyhow::bail!("Calculation Pad recovery document identity is invalid");
        }
        let stored = StoredRecovery {
            format_version: CURRENT_RECOVERY_FORMAT_VERSION,
            document_id: snapshot.document_id,
            revision: snapshot.revision,
            source: snapshot.source.clone(),
        };
        let bytes = serde_json::to_vec(&stored)
            .map_err(|error| anyhow::anyhow!("encode Calculation Pad recovery: {error}"))?;
        if bytes.len() > MAX_STORED_RECOVERY_BYTES {
            anyhow::bail!(
                "Calculation Pad recovery exceeds the {MAX_STORED_RECOVERY_BYTES}-byte stored limit"
            );
        }
        let _transaction = self.lock_transaction();
        self.ensure_write_directory()?;
        crate::common::atomic_file::save_atomic(&self.path, &bytes)
            .map_err(|error| anyhow::anyhow!("write Calculation Pad recovery: {error}"))?;
        Ok(())
    }

    fn clear(&self) -> anyhow::Result<()> {
        let _transaction = self.lock_transaction();
        if !self.ensure_existing_directory()? {
            return Ok(());
        }
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
                fs::remove_file(&self.path)
                    .map_err(|error| anyhow::anyhow!("clear Calculation Pad recovery: {error}"))?;
            }
            Ok(_) => anyhow::bail!(
                "refusing to remove unsafe Calculation Pad recovery path {}",
                self.path.display()
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "inspect Calculation Pad recovery path {}: {error}",
                    self.path.display()
                ));
            }
        }
        Ok(())
    }

    fn lock_transaction(&self) -> std::sync::MutexGuard<'_, ()> {
        self.transaction
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn ensure_existing_directory(&self) -> anyhow::Result<bool> {
        match fs::metadata(&self.root) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => anyhow::bail!("unsafe Calculation Pad recovery root"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
        match fs::symlink_metadata(&self.pad_directory) {
            Ok(metadata) if metadata.file_type().is_dir() => Ok(true),
            Ok(_) => anyhow::bail!("unsafe Calculation Pad recovery directory"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    fn ensure_write_directory(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root)?;
        match fs::symlink_metadata(&self.pad_directory) {
            Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
            Ok(_) => anyhow::bail!("unsafe Calculation Pad recovery directory"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&self.pad_directory)?;
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn validate_recovery_source(source: &str) -> anyhow::Result<()> {
    if source.len() > MAX_DOCUMENT_BYTES {
        anyhow::bail!("recovered source exceeds the {MAX_DOCUMENT_BYTES}-byte limit");
    }
    let lines = source.split('\n').count();
    if lines > MAX_SOURCE_LINES {
        anyhow::bail!("recovered source exceeds the {MAX_SOURCE_LINES}-line limit");
    }
    Ok(())
}

fn read_recovery_limited(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() {
        anyhow::bail!("unsafe Calculation Pad recovery file");
    }
    if metadata.len() > MAX_STORED_RECOVERY_BYTES as u64 {
        anyhow::bail!(
            "Calculation Pad recovery exceeds the {MAX_STORED_RECOVERY_BYTES}-byte stored limit"
        );
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)?
        .take((MAX_STORED_RECOVERY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_STORED_RECOVERY_BYTES {
        anyhow::bail!(
            "Calculation Pad recovery exceeds the {MAX_STORED_RECOVERY_BYTES}-byte stored limit"
        );
    }
    Ok(Some(bytes))
}

enum RecoveryCommand {
    Save {
        snapshot: PadRecoverySnapshot,
        due: Option<Instant>,
        reply: Option<mpsc::Sender<anyhow::Result<()>>>,
    },
    Clear {
        reply: Option<mpsc::Sender<anyhow::Result<()>>>,
    },
    Load(mpsc::Sender<anyhow::Result<Option<PadRecoverySnapshot>>>),
    Flush(mpsc::Sender<anyhow::Result<Vec<PadRecoveryWriteFailure>>>),
    Shutdown(mpsc::Sender<anyhow::Result<Vec<PadRecoveryWriteFailure>>>),
}

impl RecoveryCommand {
    fn is_fence(&self) -> bool {
        matches!(self, Self::Load(_) | Self::Flush(_) | Self::Shutdown(_))
    }
}

#[derive(Default)]
struct RecoveryQueue {
    commands: VecDeque<RecoveryCommand>,
    stopped: bool,
    async_errors: Vec<PadRecoveryWriteFailure>,
}

/// One serialized worker coalesces pending source writes while preserving
/// ordered clear/read/flush barriers.
pub struct PadRecoveryWriter {
    store: PadRecoveryStore,
    queue: Arc<(Mutex<RecoveryQueue>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl PadRecoveryWriter {
    pub fn new(store: PadRecoveryStore) -> Self {
        Self {
            store,
            queue: Arc::new((Mutex::new(RecoveryQueue::default()), Condvar::new())),
            worker: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_before_write_hook(&mut self, hook: Arc<dyn Fn() + Send + Sync>) {
        self.store.test_before_write = Some(hook);
    }

    pub fn schedule(&mut self, snapshot: PadRecoverySnapshot) -> anyhow::Result<()> {
        self.enqueue_mutation(RecoveryCommand::Save {
            snapshot,
            due: Some(Instant::now() + RECOVERY_WRITE_DEBOUNCE),
            reply: None,
        })
    }

    pub fn save_now(&mut self, snapshot: PadRecoverySnapshot) -> anyhow::Result<()> {
        let (sender, receiver) = mpsc::channel();
        self.enqueue_mutation(RecoveryCommand::Save {
            snapshot,
            due: None,
            reply: Some(sender),
        })?;
        receiver
            .recv()
            .map_err(|_| anyhow::anyhow!("Calculation Pad recovery writer stopped"))?
    }

    pub fn clear_now(&mut self) -> anyhow::Result<()> {
        let (sender, receiver) = mpsc::channel();
        self.enqueue_mutation(RecoveryCommand::Clear {
            reply: Some(sender),
        })?;
        receiver
            .recv()
            .map_err(|_| anyhow::anyhow!("Calculation Pad recovery writer stopped"))?
    }

    pub fn load(&mut self) -> anyhow::Result<Option<PadRecoverySnapshot>> {
        let (sender, receiver) = mpsc::channel();
        self.enqueue_fence(RecoveryCommand::Load(sender))?;
        receiver
            .recv()
            .map_err(|_| anyhow::anyhow!("Calculation Pad recovery writer stopped"))?
    }

    pub fn flush(&mut self) -> anyhow::Result<Vec<PadRecoveryWriteFailure>> {
        if self.worker.is_none() {
            return Ok(Vec::new());
        }
        let (sender, receiver) = mpsc::channel();
        self.enqueue_fence(RecoveryCommand::Flush(sender))?;
        receiver
            .recv()
            .map_err(|_| anyhow::anyhow!("Calculation Pad recovery writer stopped"))?
    }

    pub fn take_async_errors(&mut self) -> Vec<PadRecoveryWriteFailure> {
        let (state, _) = &*self.queue;
        let mut state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut state.async_errors)
    }

    pub fn shutdown(&mut self) -> anyhow::Result<Vec<PadRecoveryWriteFailure>> {
        if self.worker.is_none() {
            return Ok(Vec::new());
        }
        let (sender, receiver) = mpsc::channel();
        let enqueue = self.enqueue_fence(RecoveryCommand::Shutdown(sender));
        let worker = self
            .worker
            .take()
            .ok_or_else(|| anyhow::anyhow!("Calculation Pad recovery worker was not started"))?;
        let result = match enqueue {
            Ok(()) => receiver
                .recv()
                .map_err(|_| anyhow::anyhow!("Calculation Pad recovery writer stopped"))?,
            Err(error) => Err(error),
        };
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("Calculation Pad recovery writer panicked"))?;
        result
    }

    fn enqueue_mutation(&mut self, command: RecoveryCommand) -> anyhow::Result<()> {
        self.ensure_worker()?;
        let (state, changed) = &*self.queue;
        let mut state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.stopped {
            anyhow::bail!("Calculation Pad recovery writer is shut down");
        }
        let last_fence = state.commands.iter().rposition(RecoveryCommand::is_fence);
        state
            .commands
            .truncate(last_fence.map_or(0, |index| index + 1));
        state.commands.push_back(command);
        changed.notify_one();
        Ok(())
    }

    fn enqueue_fence(&mut self, command: RecoveryCommand) -> anyhow::Result<()> {
        self.ensure_worker()?;
        let (state, changed) = &*self.queue;
        let mut state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.stopped {
            anyhow::bail!("Calculation Pad recovery writer is shut down");
        }
        state.commands.push_back(command);
        changed.notify_one();
        Ok(())
    }

    fn ensure_worker(&mut self) -> anyhow::Result<()> {
        if self.worker.is_some() {
            return Ok(());
        }
        let store = self.store.clone();
        let queue = Arc::clone(&self.queue);
        let worker = thread::Builder::new()
            .name("calculation-pad-recovery".into())
            .spawn(move || recovery_worker(store, queue))
            .map_err(|error| anyhow::anyhow!("start Calculation Pad recovery writer: {error}"))?;
        self.worker = Some(worker);
        Ok(())
    }
}

impl Drop for PadRecoveryWriter {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn recovery_worker(store: PadRecoveryStore, shared: Arc<(Mutex<RecoveryQueue>, Condvar)>) {
    loop {
        let command = {
            let (state, changed) = &*shared;
            let mut state = state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                let Some(front) = state.commands.front() else {
                    if state.stopped {
                        return;
                    }
                    state = changed
                        .wait(state)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    continue;
                };
                if let RecoveryCommand::Save { due: Some(due), .. } = front
                    && state.commands.len() == 1
                    && *due > Instant::now()
                {
                    let remaining = due.saturating_duration_since(Instant::now());
                    let (next, _) = changed
                        .wait_timeout(state, remaining)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    state = next;
                    continue;
                }
                break state.commands.pop_front().expect("front was present");
            }
        };

        match command {
            RecoveryCommand::Save {
                snapshot, reply, ..
            } => {
                let result = store.save(&snapshot);
                finish_recovery_command(
                    &shared,
                    result,
                    reply,
                    Some((snapshot.document_id, snapshot.revision)),
                );
            }
            RecoveryCommand::Clear { reply } => {
                let result = store.clear();
                finish_recovery_command(&shared, result, reply, None);
            }
            RecoveryCommand::Load(reply) => {
                let _ = reply.send(store.load());
            }
            RecoveryCommand::Flush(reply) => {
                let _ = reply.send(recovery_barrier_result(&shared));
            }
            RecoveryCommand::Shutdown(reply) => {
                let _ = reply.send(recovery_barrier_result(&shared));
                let (state, changed) = &*shared;
                state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .stopped = true;
                changed.notify_all();
                return;
            }
        }
    }
}

fn recovery_barrier_result(
    shared: &Arc<(Mutex<RecoveryQueue>, Condvar)>,
) -> anyhow::Result<Vec<PadRecoveryWriteFailure>> {
    let (state, _) = &**shared;
    let mut state = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Ok(std::mem::take(&mut state.async_errors))
}

fn finish_recovery_command(
    shared: &Arc<(Mutex<RecoveryQueue>, Condvar)>,
    result: anyhow::Result<()>,
    reply: Option<mpsc::Sender<anyhow::Result<()>>>,
    failure_identity: Option<(DocumentId, u64)>,
) {
    if let Some(reply) = reply {
        let _ = reply.send(result);
    } else if let (Err(error), Some((document_id, revision))) = (result, failure_identity) {
        let (state, changed) = &**shared;
        state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .async_errors
            .push(PadRecoveryWriteFailure {
                document_id,
                revision,
                message: error.to_string(),
            });
        changed.notify_all();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentId(pub(super) u128);

impl DocumentId {
    pub(super) fn fresh() -> Self {
        loop {
            let id = Self(rand::random::<u128>());
            if id.0 != 0 {
                return id;
            }
        }
    }

    pub fn as_u128(self) -> u128 {
        self.0
    }

    fn file_name(self) -> String {
        format!("{:032x}.json", self.0)
    }
}

impl std::fmt::Display for DocumentId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:032x}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LineId(pub(super) u64);

impl LineId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadDocumentSummary {
    pub id: DocumentId,
    pub name: String,
    pub line_count: usize,
    pub source_bytes: usize,
}

#[derive(Debug)]
pub enum PadStoreError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    MalformedJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    SerializeJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    AtomicWrite {
        path: PathBuf,
        source: anyhow::Error,
    },
    UnsupportedVersion {
        found: u32,
        supported: u32,
    },
    InvalidDocument {
        path: PathBuf,
        reason: String,
    },
    InvalidName(String),
    NameAlreadyExists(String),
    DocumentNotFound(DocumentId),
    DiskConflict(DocumentId),
    OccupiedPath(PathBuf),
    OversizedDocument {
        path: PathBuf,
        max_bytes: usize,
    },
    TooManyDocuments {
        max_documents: usize,
    },
    UnsafePath(PathBuf),
    DuplicateStoredName(String),
}

impl std::fmt::Display for PadStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} {}: {source}",
                path.display()
            ),
            Self::MalformedJson { path, source } => {
                write!(
                    formatter,
                    "invalid document JSON at {}: {source}",
                    path.display()
                )
            }
            Self::SerializeJson { path, source } => {
                write!(formatter, "failed to encode {}: {source}", path.display())
            }
            Self::AtomicWrite { path, source } => {
                write!(formatter, "failed to save {}: {source}", path.display())
            }
            Self::UnsupportedVersion { found, supported } => write!(
                formatter,
                "document format version {found} is unsupported (current version is {supported})"
            ),
            Self::InvalidDocument { path, reason } => {
                write!(
                    formatter,
                    "invalid Calculation Pad document at {}: {reason}",
                    path.display()
                )
            }
            Self::InvalidName(name) => write!(formatter, "invalid document name: {name}"),
            Self::NameAlreadyExists(name) => {
                write!(formatter, "a document named {name:?} already exists")
            }
            Self::DocumentNotFound(id) => {
                write!(formatter, "Calculation Pad document {id} was not found")
            }
            Self::DiskConflict(id) => write!(
                formatter,
                "Calculation Pad document {id} changed or was deleted outside this session"
            ),
            Self::OccupiedPath(path) => {
                write!(
                    formatter,
                    "refusing to overwrite occupied path {}",
                    path.display()
                )
            }
            Self::OversizedDocument { path, max_bytes } => write!(
                formatter,
                "document at {} exceeds the {max_bytes}-byte limit",
                path.display()
            ),
            Self::TooManyDocuments { max_documents } => write!(
                formatter,
                "Calculation Pad document count exceeds the {max_documents}-document limit"
            ),
            Self::UnsafePath(path) => {
                write!(
                    formatter,
                    "refusing to use unsafe Calculation Pad path {}",
                    path.display()
                )
            }
            Self::DuplicateStoredName(name) => {
                write!(formatter, "multiple saved documents use the name {name:?}")
            }
        }
    }
}

impl std::error::Error for PadStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::MalformedJson { source, .. } | Self::SerializeJson { source, .. } => Some(source),
            Self::AtomicWrite { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredDocument {
    format_version: u32,
    pub id: DocumentId,
    pub name: String,
    pub source: String,
    pub line_ids: Vec<LineId>,
    pub next_line_id: u64,
}

impl StoredDocument {
    pub(super) fn new(
        id: DocumentId,
        name: String,
        source: String,
        line_ids: Vec<LineId>,
        next_line_id: u64,
    ) -> Self {
        Self {
            format_version: CURRENT_DOCUMENT_FORMAT_VERSION,
            id,
            name,
            source,
            line_ids,
            next_line_id,
        }
    }

    fn summary(&self) -> PadDocumentSummary {
        PadDocumentSummary {
            id: self.id,
            name: self.name.clone(),
            line_count: self.line_ids.len(),
            source_bytes: self.source.len(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct LoadedDocument {
    pub stored: StoredDocument,
    pub disk_sha256: [u8; 32],
}

/// App-owned document storage. Construction only records paths; it does not
/// create directories, scan the store, or read files.
#[derive(Clone)]
pub struct PadDocumentStore {
    root: PathBuf,
    pad_directory: PathBuf,
    documents_directory: PathBuf,
    transaction: Arc<Mutex<()>>,
}

impl PadDocumentStore {
    pub fn new(root: &AppDataRoot) -> Self {
        let root = root.path().to_path_buf();
        let pad_directory = root.join(CALCULATION_PAD_DIRECTORY);
        let documents_directory = pad_directory.join(DOCUMENTS_DIRECTORY);
        let transaction = path_transaction(&documents_directory);
        Self {
            root,
            pad_directory,
            documents_directory,
            transaction,
        }
    }

    pub fn directory(&self) -> &Path {
        &self.documents_directory
    }

    pub fn list(&self) -> Result<Vec<PadDocumentSummary>, PadStoreError> {
        let _transaction = self.lock_transaction();
        self.list_unlocked()
    }

    pub(super) fn open(&self, id: DocumentId) -> Result<LoadedDocument, PadStoreError> {
        let _transaction = self.lock_transaction();
        self.ensure_existing_directories()?;
        let path = self.document_path(id);
        let bytes = read_limited(&path)?.ok_or(PadStoreError::DocumentNotFound(id))?;
        let stored = decode_document(&path, &bytes, Some(id))?;
        Ok(LoadedDocument {
            stored,
            disk_sha256: digest(&bytes),
        })
    }

    pub(super) fn save_as(
        &self,
        name: &str,
        source: String,
        line_ids: Vec<LineId>,
        next_line_id: u64,
    ) -> Result<(StoredDocument, [u8; 32]), PadStoreError> {
        self.save_as_with_writer(name, source, line_ids, next_line_id, |path, bytes| {
            save_atomic(path, bytes)
        })
    }

    pub(super) fn save_as_with_writer(
        &self,
        name: &str,
        source: String,
        line_ids: Vec<LineId>,
        next_line_id: u64,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<(StoredDocument, [u8; 32]), PadStoreError> {
        self.save_as_with_id_and_writer(
            DocumentId::fresh(),
            name,
            source,
            line_ids,
            next_line_id,
            writer,
        )
    }

    fn save_as_with_id_and_writer(
        &self,
        id: DocumentId,
        name: &str,
        source: String,
        line_ids: Vec<LineId>,
        next_line_id: u64,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<(StoredDocument, [u8; 32]), PadStoreError> {
        let name = validate_name(name)?;
        validate_source_and_line_ids(&source, &line_ids, next_line_id, Path::new("<new>"))?;
        let _transaction = self.lock_transaction();
        let documents = self.list_unlocked()?;
        if documents.len() >= MAX_SAVED_DOCUMENTS {
            return Err(PadStoreError::TooManyDocuments {
                max_documents: MAX_SAVED_DOCUMENTS,
            });
        }
        if documents
            .iter()
            .any(|document| document.name.to_lowercase() == name.to_lowercase())
        {
            return Err(PadStoreError::NameAlreadyExists(name));
        }

        let path = self.document_path(id);
        if read_limited(&path)?.is_some() {
            return Err(PadStoreError::OccupiedPath(path));
        }
        let stored = StoredDocument::new(id, name, source, line_ids, next_line_id);
        let bytes = encode_document(&path, &stored)?;
        self.ensure_write_directories()?;
        writer(&path, &bytes).map_err(|source| PadStoreError::AtomicWrite {
            path: path.clone(),
            source,
        })?;
        Ok((stored, digest(&bytes)))
    }

    pub(super) fn save_existing(
        &self,
        stored: &StoredDocument,
        expected_disk_sha256: [u8; 32],
    ) -> Result<[u8; 32], PadStoreError> {
        self.save_existing_with_writer(stored, expected_disk_sha256, |path, bytes| {
            save_atomic(path, bytes)
        })
    }

    pub(super) fn save_existing_with_writer(
        &self,
        stored: &StoredDocument,
        expected_disk_sha256: [u8; 32],
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<[u8; 32], PadStoreError> {
        validate_stored_document(stored, &self.document_path(stored.id))?;
        let _transaction = self.lock_transaction();
        self.ensure_existing_directories()?;
        let path = self.document_path(stored.id);
        let current = read_limited(&path)?.ok_or(PadStoreError::DiskConflict(stored.id))?;
        if digest(&current) != expected_disk_sha256 {
            return Err(PadStoreError::DiskConflict(stored.id));
        }
        let bytes = encode_document(&path, stored)?;
        writer(&path, &bytes).map_err(|source| PadStoreError::AtomicWrite {
            path: path.clone(),
            source,
        })?;
        Ok(digest(&bytes))
    }

    fn list_unlocked(&self) -> Result<Vec<PadDocumentSummary>, PadStoreError> {
        if !self.ensure_existing_directories()? {
            return Ok(Vec::new());
        }

        let entries =
            fs::read_dir(&self.documents_directory).map_err(|source| PadStoreError::Io {
                operation: "list saved documents in",
                path: self.documents_directory.clone(),
                source,
            })?;
        let mut summaries = Vec::new();
        let mut names = HashSet::new();
        let mut scanned = 0;
        for entry in entries {
            scanned += 1;
            if scanned > MAX_DIRECTORY_ENTRIES {
                return Err(PadStoreError::TooManyDocuments {
                    max_documents: MAX_SAVED_DOCUMENTS,
                });
            }
            let entry = entry.map_err(|source| PadStoreError::Io {
                operation: "list saved documents in",
                path: self.documents_directory.clone(),
                source,
            })?;
            let Some(id) = document_id_from_filename(&entry.file_name()) else {
                continue;
            };
            if !entry
                .file_type()
                .map_err(|source| PadStoreError::Io {
                    operation: "inspect",
                    path: entry.path(),
                    source,
                })?
                .is_file()
            {
                return Err(PadStoreError::UnsafePath(entry.path()));
            }
            if summaries.len() >= MAX_SAVED_DOCUMENTS {
                return Err(PadStoreError::TooManyDocuments {
                    max_documents: MAX_SAVED_DOCUMENTS,
                });
            }
            let path = entry.path();
            let bytes = read_limited(&path)?.ok_or(PadStoreError::DocumentNotFound(id))?;
            let stored = decode_document(&path, &bytes, Some(id))?;
            let key = stored.name.to_lowercase();
            if !names.insert(key) {
                return Err(PadStoreError::DuplicateStoredName(stored.name));
            }
            summaries.push(stored.summary());
        }
        summaries.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(summaries)
    }

    fn lock_transaction(&self) -> std::sync::MutexGuard<'_, ()> {
        self.transaction
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn document_path(&self, id: DocumentId) -> PathBuf {
        self.documents_directory.join(id.file_name())
    }

    fn ensure_existing_directories(&self) -> Result<bool, PadStoreError> {
        match fs::metadata(&self.root) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Err(PadStoreError::UnsafePath(self.root.clone())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(source) => {
                return Err(PadStoreError::Io {
                    operation: "inspect",
                    path: self.root.clone(),
                    source,
                });
            }
        };
        for directory in [&self.pad_directory, &self.documents_directory] {
            match fs::symlink_metadata(directory) {
                Ok(metadata) if metadata.file_type().is_dir() => {}
                Ok(_) => return Err(PadStoreError::UnsafePath(directory.clone())),
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(source) => {
                    return Err(PadStoreError::Io {
                        operation: "inspect",
                        path: directory.clone(),
                        source,
                    });
                }
            }
        }
        Ok(true)
    }

    fn ensure_write_directories(&self) -> Result<(), PadStoreError> {
        fs::create_dir_all(&self.root).map_err(|source| PadStoreError::Io {
            operation: "create app data directory",
            path: self.root.clone(),
            source,
        })?;
        for directory in [&self.pad_directory, &self.documents_directory] {
            match fs::symlink_metadata(directory) {
                Ok(metadata) if metadata.file_type().is_dir() => {}
                Ok(_) => return Err(PadStoreError::UnsafePath(directory.clone())),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    fs::create_dir(directory).map_err(|source| PadStoreError::Io {
                        operation: "create Calculation Pad document directory",
                        path: directory.clone(),
                        source,
                    })?;
                }
                Err(source) => {
                    return Err(PadStoreError::Io {
                        operation: "inspect",
                        path: directory.clone(),
                        source,
                    });
                }
            }
        }
        if !self.ensure_existing_directories()? {
            return Err(PadStoreError::UnsafePath(self.documents_directory.clone()));
        }
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<String, PadStoreError> {
    let invalid_character = name.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
    });
    let trimmed = name.trim();
    if name.is_empty()
        || trimmed.is_empty()
        || name.len() > MAX_NAME_BYTES
        || name != trimmed
        || matches!(name, "." | "..")
        || name.ends_with('.')
        || name.ends_with(' ')
        || invalid_character
        || is_windows_device_name(name)
    {
        return Err(PadStoreError::InvalidName(name.to_owned()));
    }
    Ok(name.to_owned())
}

fn is_windows_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

fn validate_stored_document(stored: &StoredDocument, path: &Path) -> Result<(), PadStoreError> {
    if stored.format_version != CURRENT_DOCUMENT_FORMAT_VERSION {
        return Err(PadStoreError::UnsupportedVersion {
            found: stored.format_version,
            supported: CURRENT_DOCUMENT_FORMAT_VERSION,
        });
    }
    if stored.id.0 == 0 {
        return Err(invalid_document(path, "document ID must be nonzero"));
    }
    validate_name(&stored.name).map_err(|_| invalid_document(path, "document name is unsafe"))?;
    validate_source_and_line_ids(&stored.source, &stored.line_ids, stored.next_line_id, path)
}

fn validate_source_and_line_ids(
    source: &str,
    line_ids: &[LineId],
    next_line_id: u64,
    path: &Path,
) -> Result<(), PadStoreError> {
    if source.len() > MAX_DOCUMENT_BYTES {
        return Err(invalid_document(path, "source exceeds its byte limit"));
    }
    let line_count = source.split('\n').count();
    if line_count > MAX_SOURCE_LINES {
        return Err(invalid_document(path, "source exceeds its line limit"));
    }
    if line_ids.len() != line_count {
        return Err(invalid_document(
            path,
            "line ID count does not match source lines",
        ));
    }
    let mut seen = HashSet::with_capacity(line_ids.len());
    let mut maximum_id = 0;
    for id in line_ids {
        if id.0 == 0 || !seen.insert(id.0) {
            return Err(invalid_document(
                path,
                "line IDs must be unique and nonzero",
            ));
        }
        maximum_id = maximum_id.max(id.0);
    }
    if next_line_id == 0 || next_line_id <= maximum_id {
        return Err(invalid_document(
            path,
            "next line ID must be greater than all saved line IDs",
        ));
    }
    Ok(())
}

fn decode_document(
    path: &Path,
    bytes: &[u8],
    expected_id: Option<DocumentId>,
) -> Result<StoredDocument, PadStoreError> {
    if bytes.len() > MAX_STORED_DOCUMENT_BYTES {
        return Err(PadStoreError::OversizedDocument {
            path: path.to_path_buf(),
            max_bytes: MAX_STORED_DOCUMENT_BYTES,
        });
    }
    let stored: StoredDocument =
        serde_json::from_slice(bytes).map_err(|source| PadStoreError::MalformedJson {
            path: path.to_path_buf(),
            source,
        })?;
    validate_stored_document(&stored, path)?;
    if expected_id.is_some_and(|id| id != stored.id) {
        return Err(invalid_document(
            path,
            "filename ID does not match document ID",
        ));
    }
    Ok(stored)
}

fn encode_document(path: &Path, stored: &StoredDocument) -> Result<Vec<u8>, PadStoreError> {
    validate_stored_document(stored, path)?;
    let bytes =
        serde_json::to_vec_pretty(stored).map_err(|source| PadStoreError::SerializeJson {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_STORED_DOCUMENT_BYTES {
        return Err(PadStoreError::OversizedDocument {
            path: path.to_path_buf(),
            max_bytes: MAX_STORED_DOCUMENT_BYTES,
        });
    }
    Ok(bytes)
}

fn read_limited(path: &Path) -> Result<Option<Vec<u8>>, PadStoreError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(PadStoreError::Io {
                operation: "inspect",
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if !metadata.file_type().is_file() {
        return Err(PadStoreError::UnsafePath(path.to_path_buf()));
    }
    let file = File::open(path).map_err(|source| PadStoreError::Io {
        operation: "read",
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::new();
    file.take((MAX_STORED_DOCUMENT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| PadStoreError::Io {
            operation: "read",
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_STORED_DOCUMENT_BYTES {
        return Err(PadStoreError::OversizedDocument {
            path: path.to_path_buf(),
            max_bytes: MAX_STORED_DOCUMENT_BYTES,
        });
    }
    Ok(Some(bytes))
}

fn invalid_document(path: &Path, reason: &str) -> PadStoreError {
    PadStoreError::InvalidDocument {
        path: path.to_path_buf(),
        reason: reason.to_owned(),
    }
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn document_id_from_filename(name: &std::ffi::OsStr) -> Option<DocumentId> {
    let name = name.to_str()?;
    let stem = name.strip_suffix(".json")?;
    if stem.len() != 32
        || !stem
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let id = u128::from_str_radix(stem, 16).ok()?;
    (id != 0).then_some(DocumentId(id))
}

static PAD_TRANSACTIONS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();

fn path_transaction(path: &Path) -> Arc<Mutex<()>> {
    let transactions = PAD_TRANSACTIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut transactions = transactions
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    transactions
        .entry(path.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::{
        CURRENT_DOCUMENT_FORMAT_VERSION, CURRENT_RECOVERY_FORMAT_VERSION, DocumentId, LineId,
        MAX_DOCUMENT_BYTES, MAX_SAVED_DOCUMENTS, MAX_SOURCE_LINES, MAX_STORED_DOCUMENT_BYTES,
        MAX_STORED_RECOVERY_BYTES, PadDocumentStore, PadRecoverySnapshot, PadRecoveryStore,
        PadRecoveryWriter, PadStoreError,
    };
    use crate::platform::app_data::AppDataRoot;
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};

    fn fixture() -> (tempfile::TempDir, PadDocumentStore) {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path().join("app-data"));
        (directory, PadDocumentStore::new(&root))
    }

    fn recovery_fixture() -> (tempfile::TempDir, AppDataRoot, PadRecoveryStore) {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path().join("app-data"));
        let store = PadRecoveryStore::new(&root);
        (directory, root, store)
    }

    fn recovery_snapshot(source: &str, revision: u64) -> PadRecoverySnapshot {
        PadRecoverySnapshot {
            document_id: DocumentId(41),
            revision,
            source: source.to_owned(),
        }
    }

    #[test]
    fn recovery_round_trips_exact_bounded_source_and_rejects_bad_versions_and_sizes() {
        let (_directory, _root, store) = recovery_fixture();
        let snapshot = recovery_snapshot("π = 2\r\n1 / 0\r\n", 8);

        store.save(&snapshot).unwrap();

        assert_eq!(store.load().unwrap(), Some(snapshot));
        let encoded = std::fs::read(store.path()).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(value["format_version"], CURRENT_RECOVERY_FORMAT_VERSION);
        value["format_version"] = serde_json::json!(CURRENT_RECOVERY_FORMAT_VERSION + 1);
        std::fs::write(store.path(), serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            store
                .load()
                .unwrap_err()
                .to_string()
                .contains("unsupported")
        );

        std::fs::write(store.path(), vec![b' '; MAX_STORED_RECOVERY_BYTES + 1]).unwrap();
        assert!(
            store
                .load()
                .unwrap_err()
                .to_string()
                .contains("stored limit")
        );
    }

    #[test]
    fn recovery_record_never_changes_a_named_saved_pad() {
        let (_directory, root, recovery) = recovery_fixture();
        let named = PadDocumentStore::new(&root);
        let source = "saved = 10".to_owned();
        let (saved, _) = named
            .save_as("Named", source.clone(), vec![LineId(1)], 2)
            .unwrap();
        let path = named.document_path(saved.id);
        let original_bytes = std::fs::read(&path).unwrap();

        recovery.save(&recovery_snapshot("draft = 20", 3)).unwrap();

        assert_eq!(std::fs::read(path).unwrap(), original_bytes);
        assert_eq!(named.open(saved.id).unwrap().stored.source, source);
    }

    #[test]
    fn recovery_clear_barrier_follows_an_in_flight_delayed_write() {
        let (_directory, _root, store) = recovery_fixture();
        let gate = std::sync::Arc::new((Mutex::new((false, false)), Condvar::new()));
        let hook_gate = std::sync::Arc::clone(&gate);
        let mut writer = PadRecoveryWriter::new(store.clone());
        writer.set_before_write_hook(std::sync::Arc::new(move || {
            let (state, changed) = &*hook_gate;
            let mut state = state.lock().unwrap();
            state.0 = true;
            changed.notify_all();
            while !state.1 {
                state = changed.wait(state).unwrap();
            }
        }));

        writer
            .schedule(recovery_snapshot("discard me = 1", 4))
            .unwrap();
        let (state, changed) = &*gate;
        let mut state = state.lock().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !state.0 && Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (next, _) = changed.wait_timeout(state, remaining).unwrap();
            state = next;
        }
        assert!(
            state.0,
            "the recovery write did not enter its delayed section"
        );
        drop(state);

        let queue = std::sync::Arc::clone(&writer.queue);
        let writer = std::sync::Arc::new(Mutex::new(writer));
        let clear_writer = std::sync::Arc::clone(&writer);
        let clear = std::thread::spawn(move || clear_writer.lock().unwrap().clear_now());
        let (queued, queued_changed) = &*queue;
        let mut queued = queued.lock().unwrap();
        while !queued
            .commands
            .iter()
            .any(|command| matches!(command, super::RecoveryCommand::Clear { .. }))
        {
            queued = queued_changed.wait(queued).unwrap();
        }
        drop(queued);

        let release_gate = std::sync::Arc::clone(&gate);
        let release = std::thread::spawn(move || {
            let (state, changed) = &*release_gate;
            let mut state = state.lock().unwrap();
            state.1 = true;
            changed.notify_all();
        });
        clear.join().unwrap().unwrap();
        release.join().unwrap();
        assert_eq!(store.load().unwrap(), None);

        let mut writer = writer.lock().unwrap();
        writer.save_now(recovery_snapshot("", 0)).unwrap();
        assert_eq!(
            store.load().unwrap().map(|record| record.source),
            Some(String::new())
        );
        writer.shutdown().unwrap();
    }

    #[test]
    fn construction_and_empty_listing_do_not_create_or_scan_the_store() {
        let (_directory, store) = fixture();

        assert!(!store.directory().exists());
        assert!(store.list().unwrap().is_empty());
        assert!(!store.directory().exists());
    }

    #[test]
    fn saves_and_reopens_versioned_unicode_source_and_stable_line_ids() {
        let (_directory, store) = fixture();
        let source = "π = 2\n{顧客 名} = π + 1\n".to_owned();
        let line_ids = vec![LineId(1), LineId(2), LineId(3)];

        let (saved, disk_sha256) = store
            .save_as("工程 計算", source.clone(), line_ids.clone(), 4)
            .unwrap();
        let loaded = store.open(saved.id).unwrap();

        assert_eq!(saved.format_version, CURRENT_DOCUMENT_FORMAT_VERSION);
        assert_eq!(loaded.stored.id, saved.id);
        assert_eq!(loaded.stored.name, "工程 計算");
        assert_eq!(loaded.stored.source, source);
        assert_eq!(loaded.stored.line_ids, line_ids);
        assert_eq!(loaded.stored.next_line_id, 4);
        assert_eq!(loaded.disk_sha256, disk_sha256);
        assert_eq!(store.list().unwrap()[0].id, saved.id);
    }

    #[test]
    fn rejects_unsafe_names_occupied_save_as_names_and_path_escape_inputs() {
        let (_directory, store) = fixture();

        for name in [
            "",
            "   ",
            "../escape",
            "..\\escape",
            "C:\\escape",
            "CON",
            "bad/name",
        ] {
            assert!(
                matches!(
                    store.save_as(name, String::new(), vec![LineId(1)], 2),
                    Err(PadStoreError::InvalidName(_))
                ),
                "name: {name}"
            );
        }

        let (saved, _) = store
            .save_as("draft", "first".to_owned(), vec![LineId(1)], 2)
            .unwrap();
        let original_bytes = std::fs::read(store.document_path(saved.id)).unwrap();
        assert!(matches!(
            store.save_as("draft", "replacement".to_owned(), vec![LineId(1)], 2),
            Err(PadStoreError::NameAlreadyExists(_))
        ));
        assert_eq!(
            std::fs::read(store.document_path(saved.id)).unwrap(),
            original_bytes
        );
        assert!(matches!(
            store.save_as_with_id_and_writer(
                saved.id,
                "different display name",
                "replacement".to_owned(),
                vec![LineId(1)],
                2,
                |_, _| anyhow::bail!("occupied path must not reach the writer")
            ),
            Err(PadStoreError::OccupiedPath(path)) if path == store.document_path(saved.id)
        ));
        assert_eq!(
            std::fs::read(store.document_path(saved.id)).unwrap(),
            original_bytes
        );
    }

    #[test]
    fn save_as_refuses_a_document_beyond_the_store_capacity() {
        let (_directory, store) = fixture();
        std::fs::create_dir_all(store.directory()).unwrap();
        for index in 0..MAX_SAVED_DOCUMENTS {
            let id = DocumentId(index as u128 + 1);
            let path = store.document_path(id);
            let stored = super::StoredDocument::new(
                id,
                format!("document {index}"),
                String::new(),
                vec![LineId(1)],
                2,
            );
            let bytes = super::encode_document(&path, &stored).unwrap();
            std::fs::write(path, bytes).unwrap();
        }

        assert_eq!(store.list().unwrap().len(), MAX_SAVED_DOCUMENTS);
        assert!(matches!(
            store.save_as_with_writer(
                "one too many",
                String::new(),
                vec![LineId(1)],
                2,
                |_, _| anyhow::bail!("capacity must be checked before writing")
            ),
            Err(PadStoreError::TooManyDocuments { max_documents })
                if max_documents == MAX_SAVED_DOCUMENTS
        ));
        assert_eq!(store.list().unwrap().len(), MAX_SAVED_DOCUMENTS);
    }

    #[test]
    fn validates_format_version_unique_line_ids_and_source_bounds_on_open() {
        let (_directory, store) = fixture();
        let (saved, _) = store
            .save_as(
                "bounded",
                "first\nsecond".to_owned(),
                vec![LineId(1), LineId(2)],
                3,
            )
            .unwrap();
        let path = store.document_path(saved.id);

        let mut stored = store.open(saved.id).unwrap().stored;
        stored.format_version = CURRENT_DOCUMENT_FORMAT_VERSION + 1;
        std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
        assert!(matches!(
            store.open(saved.id),
            Err(PadStoreError::UnsupportedVersion { .. })
        ));

        stored.format_version = CURRENT_DOCUMENT_FORMAT_VERSION;
        let mismatched_id = if saved.id.as_u128() == u128::MAX {
            saved.id.as_u128() - 1
        } else {
            saved.id.as_u128() + 1
        };
        stored.id = DocumentId(mismatched_id);
        std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
        assert!(matches!(
            store.open(saved.id),
            Err(PadStoreError::InvalidDocument { .. })
        ));

        stored.id = saved.id;
        stored.line_ids = vec![LineId(1), LineId(1)];
        std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
        assert!(matches!(
            store.open(saved.id),
            Err(PadStoreError::InvalidDocument { .. })
        ));

        stored.line_ids = vec![LineId(1), LineId(2)];
        stored.source = vec![""; MAX_SOURCE_LINES + 1].join("\n");
        std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
        assert!(matches!(
            store.open(saved.id),
            Err(PadStoreError::InvalidDocument { .. })
        ));

        stored.source = "x".repeat(MAX_DOCUMENT_BYTES + 1);
        std::fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
        assert!(matches!(
            store.open(saved.id),
            Err(PadStoreError::InvalidDocument { .. })
        ));

        std::fs::write(&path, vec![b' '; MAX_STORED_DOCUMENT_BYTES + 1]).unwrap();
        assert!(matches!(
            store.open(saved.id),
            Err(PadStoreError::OversizedDocument { .. })
        ));
    }

    #[test]
    fn detects_external_modification_and_deletion_against_the_opened_byte_baseline() {
        let (_directory, store) = fixture();
        let (saved, baseline) = store
            .save_as("checked", "before".to_owned(), vec![LineId(1)], 2)
            .unwrap();
        let loaded = store.open(saved.id).unwrap();
        assert_eq!(loaded.disk_sha256, baseline);

        let stored = super::StoredDocument::new(
            loaded.stored.id,
            loaded.stored.name.clone(),
            "after".to_owned(),
            loaded.stored.line_ids.clone(),
            loaded.stored.next_line_id,
        );
        let path = store.document_path(saved.id);
        let external = std::fs::read(&path).unwrap();
        let mut external = external;
        external.push(b' ');
        std::fs::write(&path, &external).unwrap();
        assert!(matches!(
            store.save_existing(&stored, baseline),
            Err(PadStoreError::DiskConflict(id)) if id == saved.id
        ));

        std::fs::remove_file(&path).unwrap();
        assert!(matches!(
            store.save_existing(&stored, baseline),
            Err(PadStoreError::DiskConflict(id)) if id == saved.id
        ));
    }

    #[test]
    fn stored_filename_contains_only_the_generated_document_identifier() {
        let (_directory, store) = fixture();
        let (saved, _) = store
            .save_as(
                "nested display name",
                "source".to_owned(),
                vec![LineId(1)],
                2,
            )
            .unwrap();
        let path = store.document_path(saved.id);

        assert_eq!(path.parent(), Some(store.directory()));
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            format!("{:032x}.json", saved.id.as_u128())
        );
        assert_ne!(saved.id, DocumentId(0));
    }
}
