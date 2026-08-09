//! Shared coordination primitives for every asynchronous Diff operation.
//!
//! Workers are deliberately unaware of egui.  [`ResultSender`] invokes the
//! application's repaint callback after enqueueing an event, which keeps tests
//! deterministic and avoids passing an `egui::Context` into filesystem code.

use crate::diff::file_ops::{CopyDirection, DeleteMode};
use crate::diff::folder_compare::{EntryMetadata, FolderStatus};
use crate::diff::folder_scan::{DiscoveredEntry, RootIdentity, ScanRules};
use crate::diff::model::DiffSide;
use crate::diff::syntax::HighlightFragment;
use crate::diff::text_compare::{TextComparisonResult, TextComparisonRules};
use crate::diff::text_file::LoadedTextFile;
use crate::diff::watch::WatchEvent;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};

pub const DEFAULT_RESULT_CAPACITY: usize = 64;
pub const DEFAULT_BATCH_ENTRIES: usize = 128;
pub const IO_CANCELLATION_CHUNK: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffProgress {
    pub completed: u64,
    pub total: Option<u64>,
}

/// Identity shared by requests and results. Request ids never repeat within a
/// coordinator; generations identify the model which is allowed to consume it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobTag {
    pub request_id: u64,
    pub workspace_id: u64,
    pub view_id: u64,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LargeFileTier {
    /// Editable alignment, syntax highlighting, and intraline comparison.
    Normal,
    /// Still editable/comparable, with syntax and intraline work reduced.
    Large,
    /// Bounded read-only comparison; this is not a hex editor.
    Extreme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LargeFilePolicy {
    pub large_bytes: u64,
    pub extreme_bytes: u64,
    pub large_estimated_rows: u64,
    pub extreme_estimated_rows: u64,
}

impl Default for LargeFilePolicy {
    fn default() -> Self {
        Self {
            large_bytes: 8 * 1024 * 1024,
            extreme_bytes: 128 * 1024 * 1024,
            large_estimated_rows: 200_000,
            extreme_estimated_rows: 2_000_000,
        }
    }
}

impl LargeFilePolicy {
    pub fn tier(&self, bytes: u64, estimated_rows: u64) -> LargeFileTier {
        if bytes >= self.extreme_bytes || estimated_rows >= self.extreme_estimated_rows {
            LargeFileTier::Extreme
        } else if bytes >= self.large_bytes || estimated_rows >= self.large_estimated_rows {
            LargeFileTier::Large
        } else {
            LargeFileTier::Normal
        }
    }
}

impl LargeFileTier {
    pub fn editable(self) -> bool {
        self != Self::Extreme
    }
    pub fn syntax_enabled(self) -> bool {
        self == Self::Normal
    }
    pub fn intraline_enabled(self) -> bool {
        self == Self::Normal
    }
    pub fn explanation(self) -> &'static str {
        match self {
            Self::Normal => "Full editable comparison",
            Self::Large => "Large file: syntax and intraline comparison are reduced",
            Self::Extreme => "Extremely large file: bounded read-only comparison (no hex editing)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FilterIdentity {
    pub revision: u64,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum DiffRequest {
    Load {
        tag: JobTag,
        side: DiffSide,
        path: PathBuf,
        tier_policy: LargeFilePolicy,
    },
    CompareText {
        tag: JobTag,
        left: Arc<str>,
        right: Arc<str>,
        key: TextResultKey,
        rules: TextComparisonRules,
    },
    ScanFolder {
        tag: JobTag,
        root: RootIdentity,
        filter: FilterIdentity,
        rules: ScanRules,
    },
    RefineContent {
        tag: JobTag,
        key: FolderContentKey,
        rules: TextComparisonRules,
    },
    Syntax {
        tag: JobTag,
        key: SyntaxKey,
        source: Arc<str>,
    },
    Reload {
        tag: JobTag,
        event: WatchEvent,
    },
    Copy {
        tag: JobTag,
        direction: CopyDirection,
        items: Vec<PathBuf>,
    },
    Delete {
        tag: JobTag,
        mode: DeleteMode,
        items: Vec<PathBuf>,
    },
}

impl DiffRequest {
    pub fn tag(&self) -> JobTag {
        match self {
            Self::Load { tag, .. }
            | Self::CompareText { tag, .. }
            | Self::ScanFolder { tag, .. }
            | Self::RefineContent { tag, .. }
            | Self::Syntax { tag, .. }
            | Self::Reload { tag, .. }
            | Self::Copy { tag, .. }
            | Self::Delete { tag, .. } => *tag,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DiffResult {
    Loaded {
        tag: JobTag,
        side: DiffSide,
        document_revision: u64,
        tier: LargeFileTier,
        file: Arc<LoadedTextFile>,
    },
    TextCompared {
        tag: JobTag,
        key: TextResultKey,
        tier: LargeFileTier,
        result: Arc<TextComparisonResult>,
    },
    FolderBatch {
        tag: JobTag,
        root: RootIdentity,
        filter: FilterIdentity,
        entries: Vec<DiscoveredEntry>,
        complete: bool,
    },
    ContentRefined {
        tag: JobTag,
        key: FolderContentKey,
        status: FolderStatus,
    },
    SyntaxReady {
        tag: JobTag,
        key: SyntaxKey,
        fragments: Arc<[HighlightFragment]>,
    },
    Reloaded {
        tag: JobTag,
        event: WatchEvent,
        document_revision: Option<u64>,
    },
    MutationItem {
        tag: JobTag,
        path: PathBuf,
        operation: MutationKind,
        result: Result<(), String>,
    },
    Progress {
        tag: JobTag,
        progress: DiffProgress,
    },
    Cancelled {
        tag: JobTag,
    },
    Failed {
        tag: JobTag,
        message: String,
    },
}

impl DiffResult {
    pub fn tag(&self) -> JobTag {
        match self {
            Self::Loaded { tag, .. }
            | Self::TextCompared { tag, .. }
            | Self::FolderBatch { tag, .. }
            | Self::ContentRefined { tag, .. }
            | Self::SyntaxReady { tag, .. }
            | Self::Reloaded { tag, .. }
            | Self::MutationItem { tag, .. }
            | Self::Progress { tag, .. }
            | Self::Cancelled { tag }
            | Self::Failed { tag, .. } => *tag,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationKind {
    Copy,
    Delete,
}

/// The single stale-result predicate used by all consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acceptance {
    pub workspace_id: u64,
    pub view_id: u64,
    pub generation: u64,
    pub text_key: Option<TextResultKey>,
    pub root: Option<RootIdentity>,
    pub filter: Option<FilterIdentity>,
}

impl Acceptance {
    pub fn accepts_tag(&self, tag: JobTag) -> bool {
        tag.workspace_id == self.workspace_id
            && tag.view_id == self.view_id
            && tag.generation == self.generation
    }
    pub fn accepts(&self, result: &DiffResult) -> bool {
        if !self.accepts_tag(result.tag()) {
            return false;
        }
        match result {
            DiffResult::TextCompared { key, result, .. } => {
                self.text_key.as_ref() == Some(key)
                    && result.left_revision == key.left_revision
                    && result.right_revision == key.right_revision
                    && result.rules_revision == key.rules_revision
            }
            DiffResult::FolderBatch { root, filter, .. } => {
                self.root.as_ref() == Some(root) && self.filter.as_ref() == Some(filter)
            }
            DiffResult::ContentRefined { key, .. } => {
                self.root.as_ref().is_some_and(|r| key.belongs_to(r))
            }
            DiffResult::Reloaded { event, .. } => {
                event.tag.workspace == self.workspace_id
                    && event.tag.view == self.view_id
                    && event.tag.generation == self.generation
            }
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextResultKey {
    pub left_revision: u64,
    pub right_revision: u64,
    pub rules_revision: u64,
    pub algorithm: String,
    pub tier: LargeFileTier,
    pub intraline_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SyntaxKey {
    pub document_revision: u64,
    pub language: String,
    pub theme: String,
    pub source_start_line: usize,
    pub source_end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FolderContentKey {
    pub root: RootIdentity,
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub left_metadata: EntryMetadata,
    pub right_metadata: EntryMetadata,
}
impl FolderContentKey {
    fn belongs_to(&self, root: &RootIdentity) -> bool {
        &self.root == root
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WrappedLayoutKey {
    pub comparison_revision: u64,
    pub left_revision: u64,
    pub right_revision: u64,
    pub pane_width_bits: u32,
    pub font_id: String,
    pub theme: String,
    pub wrap: bool,
}

#[derive(Debug, Clone)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    /// Workers call this between directory visits, batches, read/hash chunks,
    /// comparisons, and individual mutation items.
    pub fn checkpoint(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

/// Read with cancellation checkpoints between bounded chunks. `max_bytes` is
/// used by the extreme tier to retain only a bounded preview.
pub fn read_cancellable(
    mut reader: impl Read,
    token: &CancellationToken,
    max_bytes: Option<usize>,
) -> io::Result<Vec<u8>> {
    let limit = max_bytes.unwrap_or(usize::MAX);
    let mut bytes = Vec::with_capacity(limit.min(IO_CANCELLATION_CHUNK));
    let mut chunk = vec![0; IO_CANCELLATION_CHUNK.min(limit.max(1))];
    while bytes.len() < limit {
        token
            .checkpoint()
            .map_err(|_| io::Error::new(io::ErrorKind::Interrupted, "Diff job cancelled"))?;
        let wanted = chunk.len().min(limit - bytes.len());
        let n = reader.read(&mut chunk[..wanted])?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    Ok(bytes)
}

/// Deterministic chunked byte equality which never needs two complete copies.
pub fn byte_equal_cancellable(
    mut left: impl Read,
    mut right: impl Read,
    token: &CancellationToken,
) -> io::Result<bool> {
    let mut a = vec![0; IO_CANCELLATION_CHUNK];
    let mut b = vec![0; IO_CANCELLATION_CHUNK];
    loop {
        token
            .checkpoint()
            .map_err(|_| io::Error::new(io::ErrorKind::Interrupted, "Diff job cancelled"))?;
        let an = left.read(&mut a)?;
        let bn = right.read(&mut b)?;
        if an != bn || a[..an] != b[..bn] {
            return Ok(false);
        }
        if an == 0 {
            return Ok(true);
        }
    }
}

/// Splits high-volume results into a documented maximum without cloning the
/// entries. Callers checkpoint between calls to `next`/emissions.
pub fn bounded_batches<T>(items: Vec<T>, batch_size: usize) -> impl Iterator<Item = Vec<T>> {
    let mut items: VecDeque<T> = items.into();
    let size = batch_size.max(1);
    std::iter::from_fn(move || {
        if items.is_empty() {
            return None;
        }
        Some(items.drain(..size.min(items.len())).collect())
    })
}

/// Generates ids and invalidates replacement jobs synchronously.
pub struct JobCoordinator {
    next_request: AtomicU64,
    generation: AtomicU64,
    active: Mutex<Option<CancellationToken>>,
}
impl Default for JobCoordinator {
    fn default() -> Self {
        Self {
            next_request: AtomicU64::new(1),
            generation: AtomicU64::new(0),
            active: Mutex::new(None),
        }
    }
}
impl JobCoordinator {
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    pub fn start(&self, workspace_id: u64, view_id: u64) -> (JobTag, CancellationToken) {
        let mut active = self.active.lock().unwrap();
        if let Some(old) = active.take() {
            old.cancel();
        }
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let token = CancellationToken::new();
        *active = Some(token.clone());
        let request_id = self.next_request.fetch_add(1, Ordering::Relaxed);
        (
            JobTag {
                request_id,
                workspace_id,
                view_id,
                generation,
            },
            token,
        )
    }
    pub fn cancel_and_invalidate(&self) {
        if let Some(token) = self.active.lock().unwrap().take() {
            token.cancel();
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}
impl Drop for JobCoordinator {
    fn drop(&mut self) {
        if let Ok(Some(t)) = self.active.get_mut() {
            t.cancel();
        }
    }
}

/// Bounded result queue. Dropping the receiver makes producers release their
/// payloads rather than retaining source buffers after Diff closes.
pub struct ResultSender {
    sender: mpsc::SyncSender<DiffResult>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}
pub struct ResultReceiver {
    receiver: mpsc::Receiver<DiffResult>,
}
pub fn result_channel(
    capacity: usize,
    repaint: impl Fn() + Send + Sync + 'static,
) -> (ResultSender, ResultReceiver) {
    let (sender, receiver) = mpsc::sync_channel(capacity.max(1));
    (
        ResultSender {
            sender,
            repaint: Arc::new(repaint),
        },
        ResultReceiver { receiver },
    )
}
impl Clone for ResultSender {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            repaint: self.repaint.clone(),
        }
    }
}
impl ResultSender {
    pub fn send(&self, result: DiffResult, cancel: &CancellationToken) -> Result<(), DiffResult> {
        if cancel.is_cancelled() {
            return Err(result);
        }
        match self.sender.try_send(result) {
            Ok(()) => {
                (self.repaint)();
                Ok(())
            }
            Err(mpsc::TrySendError::Full(v)) | Err(mpsc::TrySendError::Disconnected(v)) => Err(v),
        }
    }
}
impl ResultReceiver {
    pub fn try_iter(&self) -> mpsc::TryIter<'_, DiffResult> {
        self.receiver.try_iter()
    }
}

/// Entry-and-byte bounded LRU cache used by text, syntax, content, and layout
/// owners. `weight` lets callers account for retained rows/fragments/bytes.
pub struct BoundedCache<K, V> {
    values: HashMap<K, (V, usize)>,
    order: VecDeque<K>,
    max_entries: usize,
    max_weight: usize,
    weight: usize,
}
impl<K: Eq + Hash + Clone, V> BoundedCache<K, V> {
    pub fn new(max_entries: usize, max_weight: usize) -> Self {
        Self {
            values: HashMap::new(),
            order: VecDeque::new(),
            max_entries: max_entries.max(1),
            max_weight: max_weight.max(1),
            weight: 0,
        }
    }
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.values.contains_key(key) {
            self.order.retain(|k| k != key);
            self.order.push_back(key.clone());
        }
        self.values.get(key).map(|x| &x.0)
    }
    pub fn insert(&mut self, key: K, value: V, weight: usize) {
        if let Some((_, old)) = self.values.remove(&key) {
            self.weight = self.weight.saturating_sub(old);
            self.order.retain(|k| k != &key);
        }
        self.weight = self.weight.saturating_add(weight);
        self.order.push_back(key.clone());
        self.values.insert(key, (value, weight));
        while self.values.len() > self.max_entries || self.weight > self.max_weight {
            if let Some(k) = self.order.pop_front() {
                if let Some((_, w)) = self.values.remove(&k) {
                    self.weight = self.weight.saturating_sub(w);
                }
            } else {
                break;
            }
        }
    }
    pub fn clear(&mut self) {
        self.values.clear();
        self.order.clear();
        self.weight = 0;
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_cancels_and_changes_generation() {
        let c = JobCoordinator::default();
        let (a, ca) = c.start(1, 2);
        let (b, _) = c.start(1, 2);
        assert!(ca.is_cancelled());
        assert!(b.request_id > a.request_id && b.generation > a.generation);
    }
    #[test]
    fn tiers_consider_bytes_and_rows() {
        let p = LargeFilePolicy::default();
        assert_eq!(p.tier(1, p.large_estimated_rows), LargeFileTier::Large);
        assert_eq!(p.tier(p.extreme_bytes, 1), LargeFileTier::Extreme);
        assert!(!LargeFileTier::Extreme.editable());
    }
    #[test]
    fn cache_is_bounded() {
        let mut c = BoundedCache::new(2, 10);
        c.insert(1, "a", 4);
        c.insert(2, "b", 4);
        let _ = c.get(&1);
        c.insert(3, "c", 4);
        assert!(c.get(&2).is_none());
        assert_eq!(c.len(), 2);
    }
    #[test]
    fn io_and_batches_are_cooperatively_bounded() {
        let token = CancellationToken::new();
        assert!(byte_equal_cancellable(&b"same"[..], &b"same"[..], &token).unwrap());
        assert!(!byte_equal_cancellable(&b"left"[..], &b"right"[..], &token).unwrap());
        let batches: Vec<_> = bounded_batches((0..1000).collect(), 128).collect();
        assert_eq!(batches.iter().map(Vec::len).max(), Some(128));
        token.cancel();
        assert_eq!(
            read_cancellable(&b"anything"[..], &token, None)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }
}
