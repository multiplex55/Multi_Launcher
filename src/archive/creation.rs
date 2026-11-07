//! Bounded creation drafts and staged ZIP creation.
use super::model::{raw_name_is_descendant, windows_relative_cmp};
use super::native::{
    CreationDirectoryEvidence, CreationFileEvidence, NativeArchiveDestination,
    NativeCreationDirectory, NativeCreationFile, OwnedStage,
};
use super::native_io::ArchiveIoControl;
use super::{
    ArchiveLimits, DestinationOccupancy, EntryKind, SafeRelativePath, ValidatedRelativePath,
};
use crate::file_operations::{NativeFileIdentity, ScanCancellation};
use std::{
    cmp::Ordering,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct CreationSourceId(pub u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct CreationMemberId(pub u64);
#[derive(Clone, Debug)]
pub(crate) struct CreationMember {
    pub id: CreationMemberId,
    pub source: CreationSourceId,
    pub source_path: PathBuf,
    pub destination: String,
    pub kind: EntryKind,
    pub size: u64,
    evidence: Option<CreationFileEvidence>,
    directory_identity: Option<CreationDirectoryEvidence>,
}
#[derive(Clone)]
struct SourceRoot {
    id: CreationSourceId,
    path: PathBuf,
    identity: NativeFileIdentity,
    evidence: CreationDirectoryEvidence,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CreationIssueKind {
    UnsafePath,
    Duplicate,
    FileDirectory,
    FileAncestor,
    Limit,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CreationIssue {
    pub member: CreationMemberId,
    pub related: Option<CreationMemberId>,
    pub kind: CreationIssueKind,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CreationPhase {
    Idle,
    Scanning,
    Validating,
    Writing,
    Finished,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug)]
pub(crate) struct CreationProgress {
    pub phase: CreationPhase,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub completed: usize,
    pub message: Option<String>,
}
impl Default for CreationProgress {
    fn default() -> Self {
        Self {
            phase: CreationPhase::Idle,
            input_bytes: 0,
            output_bytes: 0,
            completed: 0,
            message: None,
        }
    }
}
#[derive(Clone)]
pub(crate) struct CreationSnapshot {
    pub revision: u64,
    pub members: Arc<[CreationMember]>,
    pub issues: Arc<[CreationIssue]>,
    pub validated_revision: Option<u64>,
    pub progress: CreationProgress,
}
struct ScanBatch {
    members: Vec<CreationMember>,
    roots: Vec<SourceRoot>,
}
enum WorkerResult {
    Scan(Result<ScanBatch, String>),
    Create(Result<(), String>),
    Validate(Vec<CreationIssue>),
}
#[derive(Default)]
struct Publication {
    cancelled: bool,
}
struct Worker {
    revision: u64,
    io: ArchiveIoControl,
    publication: Arc<Mutex<Publication>>,
    join: JoinHandle<WorkerResult>,
}
#[cfg(test)]
struct CreationGate {
    reached: std::sync::mpsc::SyncSender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    fired: std::sync::atomic::AtomicBool,
}
#[cfg(test)]
impl CreationGate {
    fn new() -> (
        Arc<Self>,
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::SyncSender<()>,
    ) {
        let (reached, receiver) = std::sync::mpsc::sync_channel(1);
        let (release, wait) = std::sync::mpsc::sync_channel(1);
        (
            Arc::new(Self {
                reached,
                release: Mutex::new(wait),
                fired: std::sync::atomic::AtomicBool::new(false),
            }),
            receiver,
            release,
        )
    }
    fn reach(&self) {
        if !self.fired.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let _ = self.reached.send(());
            let _ = self
                .release
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .recv_timeout(std::time::Duration::from_secs(10));
        }
    }
}
pub(crate) struct CreationSession {
    limits: ArchiveLimits,
    revision: u64,
    next_id: u64,
    members: Arc<[CreationMember]>,
    roots: Arc<[SourceRoot]>,
    issues: Arc<[CreationIssue]>,
    validated_revision: Option<u64>,
    closed: bool,
    worker: Option<Worker>,
    progress: Arc<Mutex<CreationProgress>>,
    #[cfg(test)]
    after_input: Option<Arc<CreationGate>>,
    #[cfg(test)]
    before_publication: Option<Arc<CreationGate>>,
}
impl CreationSession {
    pub(crate) fn new(limits: ArchiveLimits) -> Self {
        Self {
            limits,
            revision: 0,
            next_id: 1,
            members: Arc::from([]),
            roots: Arc::from([]),
            issues: Arc::from([]),
            validated_revision: None,
            closed: false,
            worker: None,
            progress: Arc::new(Mutex::new(CreationProgress::default())),
            #[cfg(test)]
            after_input: None,
            #[cfg(test)]
            before_publication: None,
        }
    }
    pub(crate) fn snapshot(&self) -> CreationSnapshot {
        CreationSnapshot {
            revision: self.revision,
            members: Arc::clone(&self.members),
            issues: Arc::clone(&self.issues),
            validated_revision: self.validated_revision,
            progress: self
                .progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        }
    }
    pub(crate) fn has_active_worker(&self) -> bool {
        self.worker.is_some()
    }
    fn ensure_idle(&self) -> Result<(), String> {
        if self.closed {
            Err("session is closed".into())
        } else if self.worker.is_some() {
            Err("operation still running".into())
        } else {
            Ok(())
        }
    }
    pub(crate) fn add_sources(&mut self, revision: u64, paths: Vec<PathBuf>) -> Result<(), String> {
        self.ensure_idle()?;
        if revision != self.revision {
            return Err("stale draft revision".into());
        }
        if paths.is_empty()
            || paths.len() > self.limits.max_entries.saturating_sub(self.members.len())
        {
            return Err("source count exceeds creation limit".into());
        }
        let id = self.next_id;
        let limits = self.limits;
        let existing = self.members.len();
        let existing_bytes = self
            .members
            .iter()
            .try_fold(0u64, |n, m| n.checked_add(m.size))
            .ok_or("input byte overflow")?;
        let existing_metadata = self
            .members
            .iter()
            .try_fold(0usize, |n, m| {
                n.checked_add(m.destination.len())?
                    .checked_add(m.source_path.as_os_str().len().saturating_mul(2))
            })
            .ok_or("metadata overflow")?;
        let io = ArchiveIoControl::new(ScanCancellation::new());
        let token = io.cancellation_token();
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .phase = CreationPhase::Scanning;
        let join = thread::Builder::new()
            .name("zip-create-scan".into())
            .spawn(move || {
                WorkerResult::Scan(scan_sources(
                    paths,
                    id,
                    limits,
                    existing,
                    existing_bytes,
                    existing_metadata,
                    &token,
                ))
            })
            .map_err(|e| {
                let message = e.to_string();
                let mut progress = self.progress.lock().unwrap_or_else(|e| e.into_inner());
                progress.phase = CreationPhase::Failed;
                progress.message = Some(message.clone());
                message
            })?;
        self.worker = Some(Worker {
            revision,
            io,
            publication: Arc::new(Mutex::new(Publication::default())),
            join,
        });
        Ok(())
    }
    pub(crate) fn edit(
        &mut self,
        revision: u64,
        id: CreationMemberId,
        destination: String,
    ) -> Result<(), String> {
        self.ensure_idle()?;
        if revision != self.revision {
            return Err("stale draft revision".into());
        }
        if destination.len() > self.limits.max_name_bytes {
            return Err("member path exceeds limit".into());
        }
        let current = self
            .members
            .iter()
            .find(|m| m.id == id)
            .ok_or("unknown member")?;
        let retained = self
            .members
            .iter()
            .try_fold(0usize, |n, m| {
                n.checked_add(m.destination.len())?
                    .checked_add(m.source_path.as_os_str().len().saturating_mul(2))
            })
            .ok_or("metadata overflow")?;
        let updated = retained
            .checked_sub(current.destination.len())
            .and_then(|n| n.checked_add(destination.len()))
            .ok_or("metadata overflow")?;
        if updated > self.limits.max_retained_metadata_bytes
            || updated > self.limits.max_plan_path_bytes
        {
            return Err("creation metadata limit exceeded".into());
        }
        let next_revision = self.revision.checked_add(1).ok_or("revision exhausted")?;
        let members = Arc::make_mut(&mut self.members);
        members.iter_mut().find(|m| m.id == id).unwrap().destination = destination;
        self.revision = next_revision;
        self.issues = Arc::from([]);
        self.validated_revision = None;
        Ok(())
    }
    pub(crate) fn remove(&mut self, revision: u64, id: CreationMemberId) -> Result<(), String> {
        self.ensure_idle()?;
        if revision != self.revision {
            return Err("stale draft revision".into());
        }
        let next_revision = self.revision.checked_add(1).ok_or("revision exhausted")?;
        let source = self
            .members
            .iter()
            .find(|member| member.id == id)
            .ok_or("unknown member")?
            .source;
        let mut members = self.members.to_vec();
        members.retain(|m| m.id != id);
        let mut roots = self.roots.to_vec();
        if !members.iter().any(|member| member.source == source) {
            roots.retain(|root| root.id != source);
        }
        self.roots = roots.into();
        self.members = members.into();
        self.revision = next_revision;
        self.issues = Arc::from([]);
        self.validated_revision = None;
        Ok(())
    }
    pub(crate) fn request_validation(&mut self, revision: u64) -> Result<(), String> {
        self.ensure_idle()?;
        if revision != self.revision {
            return Err("stale draft revision".into());
        }
        let members = Arc::clone(&self.members);
        let limits = self.limits;
        let io = ArchiveIoControl::new(ScanCancellation::new());
        let join = thread::Builder::new()
            .name("zip-create-validate".into())
            .spawn(move || WorkerResult::Validate(validate_members(&members, limits)))
            .map_err(|e| e.to_string())?;
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .phase = CreationPhase::Validating;
        self.worker = Some(Worker {
            revision,
            io,
            publication: Arc::new(Mutex::new(Publication::default())),
            join,
        });
        Ok(())
    }
    pub(crate) fn close(&mut self) {
        self.closed = true;
        self.cancel();
    }
    pub(crate) fn reopen(&mut self) -> Result<(), String> {
        if self.worker.is_some() {
            return Err("operation still retiring".into());
        }
        self.closed = false;
        Ok(())
    }

    pub(crate) fn start(
        &mut self,
        revision: u64,
        output: PathBuf,
        output_limit: u64,
    ) -> Result<(), String> {
        self.ensure_idle()?;
        if revision != self.revision {
            return Err("stale draft revision".into());
        }
        if self.members.is_empty() {
            return Err("draft has no members".into());
        }
        let members = Arc::clone(&self.members);
        let roots = Arc::clone(&self.roots);
        let limits = self.limits;
        let io = ArchiveIoControl::new(ScanCancellation::new());
        let worker_io = io.clone();
        let progress = Arc::clone(&self.progress);
        let publication = Arc::new(Mutex::new(Publication::default()));
        let worker_publication = Arc::clone(&publication);
        #[cfg(test)]
        let after_input = self.after_input.take();
        #[cfg(test)]
        let before_publication = self.before_publication.take();
        *progress.lock().unwrap_or_else(|e| e.into_inner()) = CreationProgress {
            phase: CreationPhase::Writing,
            ..Default::default()
        };
        let join = thread::Builder::new()
            .name("zip-create-write".into())
            .spawn(move || {
                let issues = validate_members(&members, limits);
                if !issues.is_empty() {
                    return WorkerResult::Validate(issues);
                }
                WorkerResult::Create(create_zip(
                    &members,
                    &roots,
                    &output,
                    output_limit,
                    limits,
                    &worker_io,
                    &progress,
                    &worker_publication,
                    #[cfg(test)]
                    after_input.as_deref(),
                    #[cfg(test)]
                    before_publication.as_deref(),
                ))
            })
            .map_err(|e| {
                let message = e.to_string();
                let mut progress = self.progress.lock().unwrap_or_else(|e| e.into_inner());
                progress.phase = CreationPhase::Failed;
                progress.message = Some(message.clone());
                message
            })?;
        self.worker = Some(Worker {
            revision,
            io,
            publication,
            join,
        });
        Ok(())
    }
    pub(crate) fn cancel(&self) {
        if let Some(worker) = &self.worker {
            let mut state = worker.publication.lock().unwrap_or_else(|e| e.into_inner());
            state.cancelled = true;
            worker.io.cancel();
        }
    }
    pub(crate) fn poll(&mut self) -> bool {
        if !self.worker.as_ref().is_some_and(|w| w.join.is_finished()) {
            return false;
        }
        let worker = self.worker.take().unwrap();
        let cancelled = worker.io.is_cancelled();
        let result = worker
            .join
            .join()
            .unwrap_or_else(|_| WorkerResult::Create(Err("creation worker panicked".into())));
        let mut progress = self.progress.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            WorkerResult::Scan(Ok(batch))
                if worker.revision == self.revision && !cancelled && !self.closed =>
            {
                let merged = (|| -> Result<_, String> {
                    let revision = self.revision.checked_add(1).ok_or("revision exhausted")?;
                    let next_id = batch
                        .members
                        .iter()
                        .map(|m| m.id.0)
                        .max()
                        .unwrap_or(self.next_id)
                        .checked_add(1)
                        .ok_or("identity exhausted")?;
                    let mut members = Vec::new();
                    members
                        .try_reserve_exact(
                            self.members
                                .len()
                                .checked_add(batch.members.len())
                                .ok_or("member count overflow")?,
                        )
                        .map_err(|_| "creation allocation failed")?;
                    members.extend(self.members.iter().cloned());
                    members.extend(batch.members);
                    let mut roots = Vec::new();
                    roots
                        .try_reserve_exact(
                            self.roots
                                .len()
                                .checked_add(batch.roots.len())
                                .ok_or("root count overflow")?,
                        )
                        .map_err(|_| "creation allocation failed")?;
                    roots.extend(self.roots.iter().cloned());
                    roots.extend(batch.roots);
                    Ok((revision, next_id, members, roots))
                })();
                match merged {
                    Ok((revision, next_id, members, roots)) => {
                        self.next_id = next_id;
                        self.members = members.into();
                        self.roots = roots.into();
                        self.issues = Arc::from([]);
                        self.validated_revision = None;
                        self.revision = revision;
                        progress.phase = CreationPhase::Idle;
                        progress.message = None;
                    }
                    Err(message) => {
                        progress.phase = CreationPhase::Failed;
                        progress.message = Some(message);
                    }
                }
            }
            WorkerResult::Scan(Ok(_)) => {
                progress.phase = CreationPhase::Cancelled;
            }
            WorkerResult::Scan(Err(e)) | WorkerResult::Create(Err(e)) => {
                progress.phase = if cancelled {
                    CreationPhase::Cancelled
                } else {
                    CreationPhase::Failed
                };
                progress.message = Some(e);
            }
            WorkerResult::Validate(issues) => {
                if worker.revision == self.revision && !cancelled && !self.closed {
                    self.issues = issues.into();
                    self.validated_revision = Some(worker.revision);
                    progress.phase = if self.issues.is_empty() {
                        CreationPhase::Idle
                    } else {
                        CreationPhase::Failed
                    };
                    progress.message = None;
                } else {
                    progress.phase = CreationPhase::Cancelled;
                }
            }
            WorkerResult::Create(Ok(())) => {
                progress.phase = CreationPhase::Finished;
                progress.message = None;
            }
        }
        true
    }
    pub(crate) fn shutdown(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join.join();
        }
    }
}
impl Drop for CreationSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn scan_sources(
    paths: Vec<PathBuf>,
    mut next: u64,
    limits: ArchiveLimits,
    existing: usize,
    mut total: u64,
    mut retained: usize,
    cancel: &ScanCancellation,
) -> Result<ScanBatch, String> {
    let mut members = Vec::new();
    let mut roots = Vec::new();
    let mut work = 0usize;
    for path in paths {
        let source = CreationSourceId(next);
        next = next.checked_add(1).ok_or("identity exhausted")?;
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("source name is not UTF-8")?
            .to_owned();
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        retained = retained
            .checked_add(name.len())
            .and_then(|n| n.checked_add(path.as_os_str().len().saturating_mul(2)))
            .ok_or("metadata limit overflow")?;
        if retained > limits.max_retained_metadata_bytes {
            return Err("creation metadata limit exceeded".into());
        }
        let mut stack = vec![(path.clone(), name, 1usize)];
        if metadata.is_dir() {
            let root = NativeCreationDirectory::open(&path).map_err(|e| e.to_string())?;
            roots.push(SourceRoot {
                id: source,
                path: path.clone(),
                identity: root.identity(),
                evidence: root.evidence(),
            });
        }
        while let Some((path, destination, depth)) = stack.pop() {
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
            work = work.checked_add(1).ok_or("scan work overflow")?;
            if work > limits.max_plan_work
                || depth > limits.max_path_depth
                || existing + members.len() >= limits.max_entries
            {
                return Err("creation scan limit exceeded".into());
            }
            if destination.len() > limits.max_name_bytes {
                return Err("creation metadata limit exceeded".into());
            }
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err("reparse source is unsupported".into());
            }
            let (kind, size, evidence, directory_identity) = if meta.is_dir() {
                let directory = NativeCreationDirectory::open(&path).map_err(|e| e.to_string())?;
                for item in fs::read_dir(&path).map_err(|e| e.to_string())? {
                    if cancel.is_cancelled() {
                        return Err("cancelled".into());
                    }
                    work = work.checked_add(1).ok_or("scan work overflow")?;
                    if work > limits.max_plan_work
                        || existing + members.len() + stack.len() >= limits.max_entries
                    {
                        return Err("creation scan limit exceeded".into());
                    }
                    let item = item.map_err(|e| e.to_string())?;
                    let name = item
                        .file_name()
                        .into_string()
                        .map_err(|_| "source name is not UTF-8")?;
                    let child_path = item.path();
                    let child_len = destination
                        .len()
                        .checked_add(1)
                        .and_then(|n| n.checked_add(name.len()))
                        .ok_or("metadata limit overflow")?;
                    retained = retained
                        .checked_add(child_len)
                        .and_then(|n| n.checked_add(child_path.as_os_str().len().saturating_mul(2)))
                        .ok_or("metadata limit overflow")?;
                    if retained > limits.max_retained_metadata_bytes
                        || child_len > limits.max_name_bytes
                    {
                        return Err("creation metadata limit exceeded".into());
                    }
                    stack
                        .try_reserve(1)
                        .map_err(|_| "creation allocation failed")?;
                    stack.push((child_path, format!("{destination}/{name}"), depth + 1));
                }
                directory.verify().map_err(|e| e.to_string())?;
                (EntryKind::Directory, 0, None, Some(directory.evidence()))
            } else {
                let file = NativeCreationFile::open(&path).map_err(|e| e.to_string())?;
                total = total.checked_add(file.len()).ok_or("input byte overflow")?;
                if file.len() > limits.max_actual_entry_output_bytes
                    || total > limits.max_actual_total_output_bytes
                {
                    return Err("creation input quota exceeded".into());
                }
                (EntryKind::File, file.len(), Some(file.evidence()), None)
            };
            members
                .try_reserve(1)
                .map_err(|_| "creation allocation failed")?;
            members.push(CreationMember {
                id: CreationMemberId(next),
                source,
                source_path: path,
                destination,
                kind,
                size,
                evidence,
                directory_identity,
            });
            next = next.checked_add(1).ok_or("identity exhausted")?;
        }
    }
    Ok(ScanBatch { members, roots })
}

fn validate_members(members: &[CreationMember], limits: ArchiveLimits) -> Vec<CreationIssue> {
    let mut issues = Vec::new();
    let mut sorted = Vec::new();
    if sorted.try_reserve_exact(members.len()).is_err() {
        if let Some(member) = members.first() {
            issues.push(CreationIssue {
                member: member.id,
                related: None,
                kind: CreationIssueKind::Limit,
            });
        }
        return issues;
    }
    for member in members {
        match SafeRelativePath::validate(&member.destination, member.kind, &limits) {
            Ok(path) => sorted.push((member, path.as_str())),
            Err(_) => issues.push(CreationIssue {
                member: member.id,
                related: None,
                kind: CreationIssueKind::UnsafePath,
            }),
        }
    }
    sorted.sort_by(|a, b| windows_relative_cmp(a.1, b.1));
    for pair in sorted.windows(2) {
        let ((a, ap), (b, bp)) = (pair[0], pair[1]);
        let kind = if windows_relative_cmp(ap, bp) == Ordering::Equal {
            Some(if a.kind == b.kind {
                CreationIssueKind::Duplicate
            } else {
                CreationIssueKind::FileDirectory
            })
        } else if a.kind == EntryKind::File && raw_name_is_descendant(bp.as_bytes(), ap) {
            Some(CreationIssueKind::FileAncestor)
        } else {
            None
        };
        if let Some(kind) = kind {
            issues.push(CreationIssue {
                member: b.id,
                related: Some(a.id),
                kind,
            });
        }
    }
    issues
}

struct StageWriter<'a> {
    stage: &'a OwnedStage,
    io: &'a ArchiveIoControl,
    offset: u64,
    limit: u64,
    progress: &'a Arc<Mutex<CreationProgress>>,
}
impl Write for StageWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.io.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let count = bytes.len().min(64 * 1024);
        let end = self
            .offset
            .checked_add(count as u64)
            .ok_or_else(|| io::Error::other("output counter overflow"))?;
        if end > self.limit {
            return Err(io::Error::other("ZIP output quota exceeded"));
        }
        let written = self
            .stage
            .write_chunk(self.io, self.offset, &bytes[..count])
            .map_err(io::Error::other)?;
        if written == 0 || written > count {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "stage write made invalid progress",
            ));
        }
        self.offset += written as u64;
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .output_bytes = self.offset;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.io.is_cancelled() {
            Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"))
        } else {
            Ok(())
        }
    }
}

fn create_zip(
    members: &[CreationMember],
    roots: &[SourceRoot],
    output: &Path,
    output_limit: u64,
    limits: ArchiveLimits,
    io: &ArchiveIoControl,
    progress: &Arc<Mutex<CreationProgress>>,
    publication: &Arc<Mutex<Publication>>,
    #[cfg(test)] after_input: Option<&CreationGate>,
    #[cfg(test)] before_publication: Option<&CreationGate>,
) -> Result<(), String> {
    if output
        .extension()
        .and_then(|s| s.to_str())
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("zip"))
    {
        return Err("output must be a new .zip file".into());
    }
    let leaf = output
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("output name is not UTF-8")?;
    let parent_path = output.parent().ok_or("output has no parent")?;
    let safe = SafeRelativePath::validate(leaf, EntryKind::File, &limits)
        .map_err(|e| format!("unsafe output name: {e:?}"))?;
    let relative = ValidatedRelativePath::from_safe(safe, &limits)
        .map_err(|e| format!("output path: {e:?}"))?;
    let destination = NativeArchiveDestination::open(parent_path).map_err(|e| e.to_string())?;
    let mut held_roots = Vec::new();
    held_roots
        .try_reserve_exact(roots.len())
        .map_err(|_| "source root allocation failed")?;
    for root in roots {
        let dir = NativeCreationDirectory::open(&root.path).map_err(|e| e.to_string())?;
        if dir.evidence() != root.evidence {
            return Err("source directory changed".into());
        }
        if destination.contains_ancestor(root.identity) {
            return Err("output is inside a selected source folder".into());
        }
        held_roots.push(dir);
    }
    let verify_roots = || -> Result<(), String> {
        for dir in &held_roots {
            dir.verify().map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    verify_roots()?;
    if destination
        .probe_occupancy(&relative)
        .map_err(|e| format!("output probe: {e:?}"))?
        != DestinationOccupancy::Vacant
    {
        return Err("output already exists".into());
    }
    let (parent, leaf) = destination
        .open_parent_for(&relative)
        .map_err(|e| e.to_string())?;
    if io.is_cancelled() {
        return Err("cancelled".into());
    }
    let stage = parent.create_stage().map_err(|e| e.to_string())?;
    let attempt = (|| -> Result<(), String> {
        let adapter = StageWriter {
            stage: &stage,
            io,
            offset: 0,
            limit: output_limit,
            progress,
        };
        let mut writer = zip::ZipWriter::new_stream(adapter);
        for member in members {
            if io.is_cancelled() {
                return Err("cancelled".into());
            }
            let safe = SafeRelativePath::validate(&member.destination, member.kind, &limits)
                .map_err(|e| format!("unsafe member path: {e:?}"))?;
            let mut name = safe.as_str().replace('\\', "/");
            if member.kind == EntryKind::Directory {
                let directory = NativeCreationDirectory::open(&member.source_path)
                    .map_err(|e| e.to_string())?;
                if Some(directory.evidence()) != member.directory_identity {
                    return Err("source directory changed".into());
                }
                directory.verify().map_err(|e| e.to_string())?;
                name.push('/');
                writer
                    .add_directory(name, zip::write::SimpleFileOptions::default())
                    .map_err(|e| e.to_string())?;
            } else {
                let file =
                    NativeCreationFile::open(&member.source_path).map_err(|e| e.to_string())?;
                if Some(file.evidence()) != member.evidence {
                    return Err("source file changed".into());
                }
                file.verify().map_err(|e| e.to_string())?;
                writer
                    .start_file(
                        name,
                        zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Deflated)
                            .large_file(true),
                    )
                    .map_err(|e| e.to_string())?;
                let mut offset = 0u64;
                while offset < member.size {
                    if io.is_cancelled() {
                        return Err("cancelled".into());
                    }
                    let request = (member.size - offset).min(64 * 1024) as usize;
                    let (bytes, count) = io
                        .read_at(file.verified_file(), offset, request)
                        .map_err(|e| e.to_string())?;
                    if count == 0 || count > request || count > bytes.len() {
                        return Err("source read was truncated or invalid".into());
                    }
                    writer
                        .write_all(&bytes[..count])
                        .map_err(|e| e.to_string())?;
                    offset += count as u64;
                    let mut p = progress.lock().unwrap_or_else(|e| e.into_inner());
                    p.input_bytes = p
                        .input_bytes
                        .checked_add(count as u64)
                        .ok_or("input counter overflow")?;
                    if p.input_bytes > limits.max_actual_total_output_bytes {
                        return Err("input quota exceeded".into());
                    }
                    drop(p);
                    #[cfg(test)]
                    if let Some(gate) = after_input {
                        gate.reach();
                    }
                }
                file.verify().map_err(|e| e.to_string())?;
            }
            progress.lock().unwrap_or_else(|e| e.into_inner()).completed += 1;
        }
        let adapter = writer.finish().map_err(|e| e.to_string())?.into_inner();
        let output_len = adapter.offset;
        drop(adapter);
        if io.is_cancelled() {
            return Err("cancelled".into());
        }
        let token = io.cancellation_token();
        let mut reader =
            super::native_io::OverlappedArchiveReader::new(stage.verified_file(), io, output_len);
        let catalog =
            super::read_catalog(&mut reader, super::ArchiveRevision::new(1), limits, &token)
                .map_err(|e| format!("created ZIP exceeds catalogue limits: {e:?}"))?;
        if catalog.entries().len() != members.len() {
            return Err("created ZIP catalogue has a different member count".into());
        }
        let selection = super::ArchiveSelection::all_physical(&catalog)
            .map_err(|e| format!("created ZIP selection exceeds limits: {e:?}"))?;
        super::build_extraction_plan(&catalog, &selection, None, &token)
            .map_err(|e| format!("created ZIP cannot be extracted under current limits: {e:?}"))?;
        stage.flush_final().map_err(|e| e.to_string())?;
        if io.is_cancelled() {
            return Err("cancelled".into());
        }
        verify_roots()?;
        for member in members {
            if io.is_cancelled() {
                return Err("cancelled".into());
            }
            match member.kind {
                EntryKind::File => {
                    let file =
                        NativeCreationFile::open(&member.source_path).map_err(|e| e.to_string())?;
                    if Some(file.evidence()) != member.evidence {
                        return Err("source file changed before publication".into());
                    }
                    file.verify().map_err(|e| e.to_string())?;
                }
                EntryKind::Directory => {
                    let dir = NativeCreationDirectory::open(&member.source_path)
                        .map_err(|e| e.to_string())?;
                    if Some(dir.evidence()) != member.directory_identity {
                        return Err("source directory changed before publication".into());
                    }
                    dir.verify().map_err(|e| e.to_string())?;
                }
            }
        }
        destination.verify_identity().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(error) = attempt {
        return match stage.abort() {
            Ok(()) => Err(error),
            Err(cleanup) => Err(format!("{error}; owned stage cleanup failed: {cleanup}")),
        };
    }
    #[cfg(test)]
    if let Some(gate) = before_publication {
        gate.reach();
    }
    {
        let state = publication.lock().unwrap_or_else(|e| e.into_inner());
        if state.cancelled || io.is_cancelled() {
            drop(state);
            return match stage.abort() {
                Ok(()) => Err("cancelled".into()),
                Err(e) => Err(format!("cancelled; owned stage cleanup failed: {e}")),
            };
        }
        // Passing this locked check admits publication; later cancellation
        // cannot revoke the already-started no-clobber rename.
    }
    stage.publish(&leaf).map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::{ArchiveSession, ArchiveSessionPhase};
    use std::time::{Duration, Instant};
    fn settle(session: &mut CreationSession) -> CreationSnapshot {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !session.poll() {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        session.snapshot()
    }
    fn settle_extract(session: &mut ArchiveSession) -> super::super::ArchiveSessionSnapshot {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            session.poll();
            let snapshot = session.snapshot();
            if !matches!(
                snapshot.phase,
                ArchiveSessionPhase::Opening
                    | ArchiveSessionPhase::PreparingPreview
                    | ArchiveSessionPhase::Extracting
                    | ArchiveSessionPhase::Closing
            ) {
                return snapshot;
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
    }
    #[test]
    fn creation_multiroot_unicode_empty_directory_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let folder = temp.path().join("資料");
        let dest = temp.path().join("unpacked");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        fs::create_dir(&folder).unwrap();
        fs::create_dir(folder.join("empty")).unwrap();
        fs::create_dir(&dest).unwrap();
        let a = first.join("one.txt");
        let b = second.join("two.txt");
        let c = folder.join("内容.txt");
        fs::write(&a, b"one").unwrap();
        fs::write(&b, b"two").unwrap();
        fs::write(&c, b"nested").unwrap();
        let mut creation = CreationSession::new(ArchiveLimits::default());
        creation.add_sources(0, vec![a.clone(), b.clone()]).unwrap();
        let scanned = settle(&mut creation);
        assert_eq!(scanned.members.len(), 2, "{:?}", scanned.progress.message);
        creation
            .add_sources(scanned.revision, vec![folder.clone()])
            .unwrap();
        let scanned = settle(&mut creation);
        assert_eq!(scanned.members.len(), 5, "{:?}", scanned.progress.message);
        creation
            .request_validation(creation.snapshot().revision)
            .unwrap();
        assert!(settle(&mut creation).issues.is_empty());
        let output = temp.path().join("created.zip");
        creation
            .start(scanned.revision, output.clone(), 4 * 1024 * 1024)
            .unwrap();
        let written = settle(&mut creation);
        assert_eq!(
            written.progress.phase,
            CreationPhase::Finished,
            "{:?}",
            written.progress.message
        );
        let mut extract = ArchiveSession::new(ArchiveLimits::default());
        extract.request_open(output).unwrap();
        let opened = settle_extract(&mut extract);
        assert_eq!(opened.phase, ArchiveSessionPhase::Ready);
        assert_eq!(opened.selection.len(), 5);
        extract.select_destination(dest.clone()).unwrap();
        extract.request_preview().unwrap();
        let preview = settle_extract(&mut extract);
        assert_eq!(
            preview.phase,
            ArchiveSessionPhase::PreviewReady,
            "{:?}",
            preview.failure
        );
        extract.confirm(preview.preview.unwrap().token).unwrap();
        let result = settle_extract(&mut extract);
        assert_eq!(result.phase, ArchiveSessionPhase::Finished);
        assert_eq!(fs::read(dest.join("one.txt")).unwrap(), b"one");
        assert_eq!(fs::read(dest.join("two.txt")).unwrap(), b"two");
        assert_eq!(fs::read(dest.join("資料/内容.txt")).unwrap(), b"nested");
        assert!(dest.join("資料/empty").is_dir());
        assert_eq!(fs::read(c).unwrap(), b"nested");
    }
    #[test]
    fn creation_conflicts_edits_and_rejects_existing_or_inside_output() {
        let temp = tempfile::tempdir().unwrap();
        let left = temp.path().join("left");
        let right = temp.path().join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        let a = left.join("same.txt");
        let b = right.join("SAME.txt");
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();
        let mut creation = CreationSession::new(ArchiveLimits::default());
        creation.add_sources(0, vec![a, b]).unwrap();
        let scan = settle(&mut creation);
        creation.request_validation(scan.revision).unwrap();
        assert_eq!(
            settle(&mut creation).issues[0].kind,
            CreationIssueKind::Duplicate
        );
        creation
            .edit(scan.revision, scan.members[1].id, "renamed.txt".into())
            .unwrap();
        creation
            .request_validation(creation.snapshot().revision)
            .unwrap();
        assert!(settle(&mut creation).issues.is_empty());
        let revision = creation.snapshot().revision;
        creation
            .edit(revision, scan.members[0].id, "path".into())
            .unwrap();
        creation
            .edit(
                creation.snapshot().revision,
                scan.members[1].id,
                "path/child".into(),
            )
            .unwrap();
        creation
            .request_validation(creation.snapshot().revision)
            .unwrap();
        assert_eq!(
            settle(&mut creation).issues[0].kind,
            CreationIssueKind::FileAncestor
        );
        creation
            .edit(
                creation.snapshot().revision,
                scan.members[0].id,
                "one.txt".into(),
            )
            .unwrap();
        creation
            .edit(
                creation.snapshot().revision,
                scan.members[1].id,
                "renamed.txt".into(),
            )
            .unwrap();
        let existing = temp.path().join("existing.zip");
        fs::write(&existing, b"winner").unwrap();
        creation
            .start(creation.snapshot().revision, existing.clone(), 1024 * 1024)
            .unwrap();
        assert_eq!(settle(&mut creation).progress.phase, CreationPhase::Failed);
        assert_eq!(fs::read(existing).unwrap(), b"winner");
        let mut folder = CreationSession::new(ArchiveLimits::default());
        folder.add_sources(0, vec![left.clone()]).unwrap();
        let scan = settle(&mut folder);
        folder
            .start(scan.revision, left.join("inside.zip"), 1024 * 1024)
            .unwrap();
        assert_eq!(settle(&mut folder).progress.phase, CreationPhase::Failed);
        assert!(!left.join("inside.zip").exists());
    }
    #[test]
    fn creation_rejects_changed_source_and_aborts_stage() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.txt");
        fs::write(&source, b"before").unwrap();
        let fixed_time = std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        fs::OpenOptions::new()
            .write(true)
            .open(&source)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(fixed_time))
            .unwrap();
        let mut session = CreationSession::new(ArchiveLimits::default());
        session.add_sources(0, vec![source.clone()]).unwrap();
        let scanned = settle(&mut session);
        let initial_modified = fs::metadata(&source).unwrap().modified().unwrap();
        fs::write(&source, b"after!").unwrap();
        let file = fs::OpenOptions::new().write(true).open(&source).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(fixed_time))
            .unwrap();
        drop(file);
        assert_eq!(
            fs::metadata(&source).unwrap().modified().unwrap(),
            initial_modified
        );
        assert_ne!(
            Some(NativeCreationFile::open(&source).unwrap().evidence()),
            scanned.members[0].evidence
        );
        let output = temp.path().join("out.zip");
        session
            .start(scanned.revision, output.clone(), 1024 * 1024)
            .unwrap();
        let result = settle(&mut session);
        assert_eq!(result.progress.phase, CreationPhase::Failed);
        assert!(!output.exists());
        assert!(
            !fs::read_dir(temp.path()).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".stage"))
        );
    }

    #[test]
    fn creation_detects_folder_children_changed_after_scan() {
        use std::os::windows::fs::OpenOptionsExt;
        for remove in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let folder = temp.path().join("input");
            fs::create_dir(&folder).unwrap();
            let child = folder.join("child.txt");
            if remove {
                fs::write(&child, b"child").unwrap();
            }
            let handle = fs::OpenOptions::new()
                .access_mode(0x100)
                .custom_flags(0x0200_0000)
                .open(&folder)
                .unwrap();
            handle
                .set_times(
                    fs::FileTimes::new()
                        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000)),
                )
                .unwrap();
            drop(handle);
            let mut session = CreationSession::new(ArchiveLimits::default());
            session.add_sources(0, vec![folder]).unwrap();
            let scan = settle(&mut session);
            if remove {
                fs::remove_file(&child).unwrap();
            } else {
                fs::write(&child, b"new").unwrap();
            }
            let output = temp.path().join("out.zip");
            session
                .start(scan.revision, output.clone(), 1024 * 1024)
                .unwrap();
            assert_eq!(settle(&mut session).progress.phase, CreationPhase::Failed);
            assert!(!output.exists());
        }
    }

    #[test]
    fn creation_directory_aliases_and_limits_are_transactional() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let mut session = CreationSession::new(ArchiveLimits::default());
        session
            .add_sources(0, vec![first.clone(), second.clone()])
            .unwrap();
        let scan = settle(&mut session);
        assert_eq!(scan.validated_revision, None);
        session
            .edit(scan.revision, scan.members[0].id, "folder".into())
            .unwrap();
        let revision = session.snapshot().revision;
        session
            .edit(revision, scan.members[1].id, "FOLDER/".into())
            .unwrap();
        session
            .request_validation(session.snapshot().revision)
            .unwrap();
        let validated = settle(&mut session);
        assert_eq!(validated.validated_revision, Some(validated.revision));
        assert_eq!(validated.issues[0].kind, CreationIssueKind::Duplicate);
        session
            .remove(session.snapshot().revision, scan.members[1].id)
            .unwrap();
        assert_eq!(session.snapshot().validated_revision, None);
        session
            .request_validation(session.snapshot().revision)
            .unwrap();
        assert!(settle(&mut session).issues.is_empty());
        let file = temp.path().join("file.txt");
        fs::write(&file, b"file").unwrap();
        session
            .add_sources(session.snapshot().revision, vec![file])
            .unwrap();
        let added = settle(&mut session);
        let file_id = added
            .members
            .iter()
            .find(|member| member.kind == EntryKind::File)
            .unwrap()
            .id;
        session
            .edit(added.revision, file_id, "FOLDER".into())
            .unwrap();
        session
            .request_validation(session.snapshot().revision)
            .unwrap();
        assert_eq!(
            settle(&mut session).issues[0].kind,
            CreationIssueKind::FileDirectory
        );

        let mut limits = ArchiveLimits::default();
        limits.max_entries = 1;
        let mut limited = CreationSession::new(limits);
        limited.add_sources(0, vec![first]).unwrap();
        let before = settle(&mut limited);
        assert!(limited.add_sources(before.revision, vec![second]).is_err());
        let after = limited.snapshot();
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.members.len(), 1);
    }

    #[test]
    fn creation_cancellation_and_late_competitor_leave_no_stage() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.bin");
        fs::write(&source, vec![42u8; 200_000]).unwrap();
        let mut session = CreationSession::new(ArchiveLimits::default());
        session.add_sources(0, vec![source]).unwrap();
        let scan = settle(&mut session);
        let output = temp.path().join("output.zip");
        let (gate, reached, release) = CreationGate::new();
        session.after_input = Some(gate);
        session
            .start(scan.revision, output.clone(), 1024 * 1024)
            .unwrap();
        reached.recv_timeout(Duration::from_secs(10)).unwrap();
        session.close();
        assert!(session.has_active_worker());
        assert!(
            session.reopen().is_err(),
            "retiring worker blocks a new admission"
        );
        release.send(()).unwrap();
        assert_eq!(
            settle(&mut session).progress.phase,
            CreationPhase::Cancelled
        );
        assert!(!session.has_active_worker());
        session.reopen().unwrap();
        assert!(!output.exists());
        assert!(
            !fs::read_dir(temp.path()).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".stage"))
        );
        let (gate, reached, release) = CreationGate::new();
        session.before_publication = Some(gate);
        session
            .start(session.snapshot().revision, output.clone(), 1024 * 1024)
            .unwrap();
        reached.recv_timeout(Duration::from_secs(10)).unwrap();
        session.cancel();
        release.send(()).unwrap();
        assert_eq!(
            settle(&mut session).progress.phase,
            CreationPhase::Cancelled
        );
        assert!(!output.exists());
        assert!(
            !fs::read_dir(temp.path()).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".stage"))
        );
        let (gate, reached, release) = CreationGate::new();
        session.before_publication = Some(gate);
        session
            .start(session.snapshot().revision, output.clone(), 1024 * 1024)
            .unwrap();
        reached.recv_timeout(Duration::from_secs(10)).unwrap();
        fs::write(&output, b"competitor").unwrap();
        release.send(()).unwrap();
        assert_eq!(settle(&mut session).progress.phase, CreationPhase::Failed);
        assert_eq!(fs::read(output).unwrap(), b"competitor");
        assert!(
            !fs::read_dir(temp.path()).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".stage"))
        );
    }
}
