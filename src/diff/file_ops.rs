//! The sole mutation boundary for Diff folder operations.
//!
//! Planning is intentionally immutable: UI code captures a plan once, displays
//! it, and passes that same value to execution.  Every target is checked again
//! immediately before it is mutated.  A batch is best-effort, not atomic.
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyDirection {
    LeftToRight,
    RightToLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIdentity {
    pub kind: EntryType,
    pub len: u64,
    pub modified: Option<SystemTime>,
    #[cfg(unix)]
    pub device: u64,
    #[cfg(unix)]
    pub inode: u64,
}

impl FileIdentity {
    fn read(path: &Path) -> io::Result<Self> {
        let m = fs::symlink_metadata(path)?;
        if m.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "symbolic links are not mutable Diff entries",
            ));
        }
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            kind: if m.is_dir() {
                EntryType::Directory
            } else {
                EntryType::File
            },
            len: m.len(),
            modified: m.modified().ok(),
            #[cfg(unix)]
            device: m.dev(),
            #[cfg(unix)]
            inode: m.ino(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedRoot {
    pub path: PathBuf,
    pub canonical: PathBuf,
    pub identity: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedDirectory {
    pub relative: PathBuf,
    pub target: PathBuf,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCopy {
    pub relative: PathBuf,
    pub source: PathBuf,
    pub target: PathBuf,
    pub expected_source: FileIdentity,
    pub expected_destination: Option<FileIdentity>,
    pub overwrite: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanConflict {
    pub relative: PathBuf,
    pub message: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedEntry {
    pub relative: PathBuf,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub relative: Option<PathBuf>,
    pub message: String,
    pub fatal: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PreviewTotals {
    pub files_copied: usize,
    pub overwrites: usize,
    pub directories_created: usize,
    pub conflicts: usize,
    pub skips: usize,
    pub errors: usize,
}

#[derive(Debug, Clone)]
pub struct CopyPlan {
    pub direction: CopyDirection,
    pub source_root: CapturedRoot,
    pub destination_root: CapturedRoot,
    pub generation: u64,
    pub directories: Vec<PlannedDirectory>,
    pub copies: Vec<PlannedCopy>,
    pub conflicts: Vec<PlanConflict>,
    pub skipped: Vec<SkippedEntry>,
    pub errors: Vec<ValidationError>,
    pub totals: PreviewTotals,
}
impl CopyPlan {
    pub fn has_fatal_errors(&self) -> bool {
        self.errors.iter().any(|e| e.fatal)
    }
    pub fn requires_confirmation(&self) -> bool {
        self.totals.overwrites > 0 || self.totals.files_copied > 1
    }
}

fn root(path: &Path) -> Result<CapturedRoot, String> {
    let canonical =
        fs::canonicalize(path).map_err(|e| format!("root '{}': {e}", path.display()))?;
    let identity =
        FileIdentity::read(&canonical).map_err(|e| format!("root '{}': {e}", path.display()))?;
    if identity.kind != EntryType::Directory {
        return Err(format!("root '{}' is not a directory", path.display()));
    }
    Ok(CapturedRoot {
        path: path.to_path_buf(),
        canonical,
        identity,
    })
}

/// Normalize an untrusted comparison identity. Comparison identities use `/`;
/// alternative separators are rejected rather than interpreted.
pub fn validate_relative(path: &Path) -> Result<PathBuf, String> {
    let raw = path.to_string_lossy();
    if raw.is_empty() {
        return Err("empty relative path".into());
    }
    if raw.contains('\\') {
        return Err("malformed or platform-specific separator".into());
    }
    // Windows prefixes must also be rejected when tests run on Unix.
    let b = raw.as_bytes();
    if raw.starts_with('/') || (b.len() >= 2 && b[1] == b':') || raw.starts_with("//") {
        return Err("absolute paths and platform prefixes are forbidden".into());
    }
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(v) if !v.is_empty() => out.push(v),
            _ => return Err("root, prefix, '.', and '..' components are forbidden".into()),
        }
    }
    Ok(out)
}

fn ensure_contained(root: &CapturedRoot, relative: &Path) -> Result<PathBuf, String> {
    // `relative` is already a captured identity here. On Windows a validated
    // slash-delimited selection is represented by `PathBuf` with native `\`
    // separators after components are joined during recursive expansion. Do
    // not reapply the raw-input separator rule to that internal value.
    let rel = validate_captured_relative(relative)?;
    let target = root.canonical.join(&rel);
    let mut ancestor = target.as_path();
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or("target has no existing ancestor")?;
    }
    let resolved = fs::canonicalize(ancestor).map_err(|e| e.to_string())?;
    if !resolved.starts_with(&root.canonical) {
        return Err("path escapes comparison root through a symbolic link".into());
    }
    Ok(target)
}

fn validate_captured_relative(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("empty relative path".into());
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) if !value.is_empty() => out.push(value),
            _ => return Err("root, prefix, '.', and '..' components are forbidden".into()),
        }
    }
    Ok(out)
}

pub fn plan_copy(
    source_root: &Path,
    destination_root: &Path,
    direction: CopyDirection,
    selected: impl IntoIterator<Item = PathBuf>,
    generation: u64,
) -> Result<CopyPlan, String> {
    let source_root = root(source_root)?;
    let destination_root = root(destination_root)?;
    let mut errors = vec![];
    let mut normalized = BTreeSet::new();
    for input in selected {
        match validate_relative(&input) {
            Ok(p) => {
                normalized.insert(p);
            }
            Err(message) => errors.push(ValidationError {
                relative: Some(input),
                message,
                fatal: true,
            }),
        }
    }
    let all = normalized.clone();
    normalized.retain(|p| {
        !p.ancestors()
            .skip(1)
            .filter(|a| !a.as_os_str().is_empty())
            .any(|a| all.contains(a))
    });
    let mut directories = vec![];
    let mut copies = vec![];
    let mut conflicts = vec![];
    let mut skipped = vec![];
    for rel in normalized {
        let source = match ensure_contained(&source_root, &rel) {
            Ok(v) => v,
            Err(message) => {
                errors.push(ValidationError {
                    relative: Some(rel),
                    message,
                    fatal: true,
                });
                continue;
            }
        };
        let target = match ensure_contained(&destination_root, &rel) {
            Ok(v) => v,
            Err(message) => {
                errors.push(ValidationError {
                    relative: Some(rel),
                    message,
                    fatal: true,
                });
                continue;
            }
        };
        match FileIdentity::read(&source) {
            Ok(i) if i.kind == EntryType::File => add_file(
                &source_root,
                &destination_root,
                rel,
                source,
                target,
                i,
                &mut directories,
                &mut copies,
                &mut conflicts,
                &mut skipped,
            ),
            Ok(i) if i.kind == EntryType::Directory => walk_selected(
                &source_root,
                &destination_root,
                &rel,
                &mut directories,
                &mut copies,
                &mut conflicts,
                &mut skipped,
                &mut errors,
            ),
            Ok(_) => unreachable!(),
            Err(e) => errors.push(ValidationError {
                relative: Some(rel),
                message: e.to_string(),
                fatal: true,
            }),
        }
    }
    directories.sort_by(|a, b| {
        a.relative
            .components()
            .count()
            .cmp(&b.relative.components().count())
            .then(a.relative.cmp(&b.relative))
    });
    directories.dedup_by(|a, b| a.relative == b.relative);
    copies.sort_by(|a, b| a.relative.cmp(&b.relative));
    let totals = PreviewTotals {
        files_copied: copies.len(),
        overwrites: copies.iter().filter(|x| x.overwrite).count(),
        directories_created: directories.len(),
        conflicts: conflicts.len(),
        skips: skipped.len(),
        errors: errors.len(),
    };
    Ok(CopyPlan {
        direction,
        source_root,
        destination_root,
        generation,
        directories,
        copies,
        conflicts,
        skipped,
        errors,
        totals,
    })
}

fn walk_selected(
    sr: &CapturedRoot,
    dr: &CapturedRoot,
    base: &Path,
    dirs: &mut Vec<PlannedDirectory>,
    copies: &mut Vec<PlannedCopy>,
    conflicts: &mut Vec<PlanConflict>,
    skipped: &mut Vec<SkippedEntry>,
    errors: &mut Vec<ValidationError>,
) {
    let mut stack = vec![base.to_path_buf()];
    while let Some(rel) = stack.pop() {
        let source = match ensure_contained(sr, &rel) {
            Ok(x) => x,
            Err(message) => {
                errors.push(ValidationError {
                    relative: Some(rel),
                    message,
                    fatal: true,
                });
                continue;
            }
        };
        let target = match ensure_contained(dr, &rel) {
            Ok(x) => x,
            Err(message) => {
                errors.push(ValidationError {
                    relative: Some(rel),
                    message,
                    fatal: true,
                });
                continue;
            }
        };
        match fs::symlink_metadata(&target) {
            Ok(m) if !m.is_dir() => {
                conflicts.push(PlanConflict {
                    relative: rel,
                    message: "destination is not a directory".into(),
                });
                continue;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => dirs.push(PlannedDirectory {
                relative: rel.clone(),
                target,
            }),
            Err(e) => {
                errors.push(ValidationError {
                    relative: Some(rel),
                    message: e.to_string(),
                    fatal: true,
                });
                continue;
            }
            _ => {}
        }
        let read = match fs::read_dir(&source) {
            Ok(x) => x,
            Err(e) => {
                errors.push(ValidationError {
                    relative: Some(rel),
                    message: e.to_string(),
                    fatal: true,
                });
                continue;
            }
        };
        let mut children = read.filter_map(Result::ok).collect::<Vec<_>>();
        children.sort_by_key(|x| x.file_name());
        for e in children.into_iter().rev() {
            let child = rel.join(e.file_name());
            // Reconstruct from the captured canonical root rather than keeping
            // `DirEntry::path()`. On Windows the latter can change between DOS
            // and verbatim (`\\?\`) spellings, making an unchanged source fail
            // the exact captured-target freshness check during execution.
            let child_source = sr.canonical.join(&child);
            match FileIdentity::read(&child_source) {
                Ok(i) if i.kind == EntryType::Directory => stack.push(child),
                Ok(i) => {
                    let t = dr.canonical.join(&child);
                    add_file(
                        sr,
                        dr,
                        child,
                        child_source,
                        t,
                        i,
                        dirs,
                        copies,
                        conflicts,
                        skipped,
                    )
                }
                Err(e) => skipped.push(SkippedEntry {
                    relative: child,
                    reason: e.to_string(),
                }),
            }
        }
    }
}

fn add_file(
    _sr: &CapturedRoot,
    dr: &CapturedRoot,
    rel: PathBuf,
    source: PathBuf,
    target: PathBuf,
    identity: FileIdentity,
    dirs: &mut Vec<PlannedDirectory>,
    copies: &mut Vec<PlannedCopy>,
    conflicts: &mut Vec<PlanConflict>,
    skipped: &mut Vec<SkippedEntry>,
) {
    let dest = FileIdentity::read(&target);
    match dest {
        Ok(d) if d.kind == EntryType::Directory => conflicts.push(PlanConflict {
            relative: rel,
            message: "destination is a directory".into(),
        }),
        Ok(d) if d == identity => skipped.push(SkippedEntry {
            relative: rel,
            reason: "source and destination metadata are identical".into(),
        }),
        Ok(d) => copies.push(PlannedCopy {
            relative: rel,
            source,
            target,
            expected_source: identity,
            expected_destination: Some(d),
            overwrite: true,
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = rel.parent().filter(|p| !p.as_os_str().is_empty()) {
                if !dr.canonical.join(parent).exists() {
                    dirs.push(PlannedDirectory {
                        relative: parent.into(),
                        target: dr.canonical.join(parent),
                    });
                }
            }
            copies.push(PlannedCopy {
                relative: rel,
                source,
                target,
                expected_source: identity,
                expected_destination: None,
                overwrite: false,
            })
        }
        Err(e) => conflicts.push(PlanConflict {
            relative: rel,
            message: e.to_string(),
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOutcome {
    CreatedDirectory,
    Copied,
    Overwritten,
    Failed(String),
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemResult {
    pub relative: PathBuf,
    pub outcome: ItemOutcome,
}
#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub generation: u64,
    pub items: Vec<ItemResult>,
    pub affected_subtree: Option<PathBuf>,
    pub cancelled: bool,
}

fn fresh(plan: &CopyPlan, item: &PlannedCopy, dirty: &HashSet<PathBuf>) -> Result<(), String> {
    if !same_root_identity(&plan.source_root)? || !same_root_identity(&plan.destination_root)? {
        return Err("comparison root identity changed".into());
    }
    let src = ensure_contained(&plan.source_root, &item.relative)?;
    let dst = ensure_contained(&plan.destination_root, &item.relative)?;
    if src != item.source || dst != item.target {
        return Err("captured target changed meaning".into());
    }
    if FileIdentity::read(&src).map_err(|e| e.to_string())? != item.expected_source {
        return Err("source changed since confirmation".into());
    }
    if dirty.contains(&dst) {
        return Err("destination has unsaved Diff changes".into());
    }
    let now = FileIdentity::read(&dst).ok();
    if now != item.expected_destination {
        return Err("destination type or overwrite state changed since confirmation".into());
    }
    Ok(())
}

fn same_root_identity(root: &CapturedRoot) -> Result<bool, String> {
    let now = FileIdentity::read(&root.canonical).map_err(|e| e.to_string())?;
    if now.kind != EntryType::Directory {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        Ok(now.device == root.identity.device && now.inode == root.identity.inode)
    }
    #[cfg(not(unix))]
    {
        Ok(fs::canonicalize(&root.path).map_err(|e| e.to_string())? == root.canonical)
    }
}

pub fn execute_copy(
    plan: &CopyPlan,
    dirty: &HashSet<PathBuf>,
    cancel: &AtomicBool,
) -> ExecutionReport {
    let mut items = vec![];
    let mut affected = vec![];
    if plan.has_fatal_errors() {
        return ExecutionReport {
            generation: plan.generation,
            items,
            affected_subtree: None,
            cancelled: false,
        };
    }
    for d in &plan.directories {
        if cancel.load(Ordering::Acquire) {
            return finish(plan, items, affected, true);
        }
        match ensure_contained(&plan.destination_root, &d.relative)
            .and_then(|p| {
                if p == d.target {
                    Ok(p)
                } else {
                    Err("captured directory target changed".into())
                }
            })
            // A planned descendant can have more than one missing ancestor.
            // `create_dir_all` is idempotent and makes each planned directory
            // usable even when another planned directory was deduplicated.
            .and_then(|p| fs::create_dir_all(&p).map(|_| p).map_err(|e| e.to_string()))
        {
            Ok(_) => {
                items.push(ItemResult {
                    relative: d.relative.clone(),
                    outcome: ItemOutcome::CreatedDirectory,
                });
                affected.push(d.relative.clone())
            }
            Err(e) if d.target.is_dir() => {}
            Err(e) => items.push(ItemResult {
                relative: d.relative.clone(),
                outcome: ItemOutcome::Failed(e),
            }),
        }
    }
    for c in &plan.copies {
        if cancel.load(Ordering::Acquire) {
            items.push(ItemResult {
                relative: c.relative.clone(),
                outcome: ItemOutcome::Cancelled,
            });
            return finish(plan, items, affected, true);
        }
        let outcome = match fresh(plan, c, dirty) {
            Err(e) => ItemOutcome::Failed(e),
            Ok(()) => match fs::copy(&c.source, &c.target) {
                Ok(_) => {
                    if let Ok(metadata) = fs::metadata(&c.source) {
                        let _ = fs::set_permissions(&c.target, metadata.permissions());
                    }
                    affected.push(c.relative.clone());
                    if c.overwrite {
                        ItemOutcome::Overwritten
                    } else {
                        ItemOutcome::Copied
                    }
                }
                Err(e) => ItemOutcome::Failed(e.to_string()),
            },
        };
        items.push(ItemResult {
            relative: c.relative.clone(),
            outcome,
        });
    }
    finish(plan, items, affected, false)
}
fn finish(
    plan: &CopyPlan,
    items: Vec<ItemResult>,
    affected: Vec<PathBuf>,
    cancelled: bool,
) -> ExecutionReport {
    ExecutionReport {
        generation: plan.generation,
        affected_subtree: smallest_common_subtree(&affected),
        items,
        cancelled,
    }
}
pub fn smallest_common_subtree(paths: &[PathBuf]) -> Option<PathBuf> {
    let first = paths.first()?;
    let mut out = first.parent().unwrap_or(Path::new("")).to_path_buf();
    for p in &paths[1..] {
        let p = p.parent().unwrap_or(Path::new(""));
        while !p.starts_with(&out) {
            if !out.pop() {
                break;
            }
        }
    }
    Some(out)
}

#[derive(Debug, Clone)]
pub enum OperationEvent {
    Progress {
        generation: u64,
        completed: usize,
        total: usize,
    },
    Completed(ExecutionReport),
}
pub struct OperationHandle {
    pub receiver: Receiver<OperationEvent>,
    cancel: Arc<AtomicBool>,
}
impl OperationHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release)
    }
}
pub fn spawn_copy(plan: CopyPlan, dirty: HashSet<PathBuf>) -> OperationHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let c = cancel.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let report = execute_copy(&plan, &dirty, &c);
        let _ = tx.send(OperationEvent::Completed(report));
    });
    OperationHandle {
        receiver: rx,
        cancel,
    }
}
pub fn event_is_current(event: &OperationEvent, generation: u64) -> bool {
    match event {
        OperationEvent::Progress { generation: g, .. } => *g == generation,
        OperationEvent::Completed(r) => r.generation == generation,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteMode {
    Recycle,
    Permanent,
}
#[derive(Debug, Clone)]
pub struct PlannedDelete {
    pub relative: PathBuf,
    pub target: PathBuf,
    pub expected: FileIdentity,
}
#[derive(Debug, Clone)]
pub struct DeletePlan {
    pub root: CapturedRoot,
    pub generation: u64,
    pub side: String,
    pub mode: DeleteMode,
    pub items: Vec<PlannedDelete>,
    pub errors: Vec<ValidationError>,
}
pub trait TrashBackend: Send + Sync {
    fn recycle(&self, path: &Path) -> Result<(), String>;
    fn permanently_delete(&self, path: &Path) -> Result<(), String>;
}
#[derive(Default)]
pub struct SystemTrash;
impl TrashBackend for SystemTrash {
    fn recycle(&self, p: &Path) -> Result<(), String> {
        trash::delete(p).map_err(|e| e.to_string())
    }
    fn permanently_delete(&self, p: &Path) -> Result<(), String> {
        if p.is_dir() {
            fs::remove_dir_all(p)
        } else {
            fs::remove_file(p)
        }
        .map_err(|e| e.to_string())
    }
}
pub fn plan_delete(
    root_path: &Path,
    side: impl Into<String>,
    selected: impl IntoIterator<Item = PathBuf>,
    generation: u64,
    mode: DeleteMode,
) -> Result<DeletePlan, String> {
    let root = root(root_path)?;
    let mut items = vec![];
    let mut errors = vec![];
    for rel in selected {
        match validate_relative(&rel).and_then(|validated| {
            ensure_contained(&root, &validated).and_then(|target| {
                FileIdentity::read(&target)
                    .map(|expected| PlannedDelete {
                        relative: validated,
                        target,
                        expected,
                    })
                    .map_err(|e| e.to_string())
            })
        }) {
            Ok(x) => items.push(x),
            Err(message) => errors.push(ValidationError {
                relative: Some(rel),
                message,
                fatal: true,
            }),
        }
    }
    items.sort_by(|a, b| {
        b.relative
            .components()
            .count()
            .cmp(&a.relative.components().count())
            .then(a.relative.cmp(&b.relative))
    });
    Ok(DeletePlan {
        root,
        generation,
        side: side.into(),
        mode,
        items,
        errors,
    })
}
pub fn execute_delete(
    plan: &DeletePlan,
    dirty: &HashSet<PathBuf>,
    backend: &dyn TrashBackend,
    cancel: &AtomicBool,
) -> ExecutionReport {
    let mut results = vec![];
    let mut affected = vec![];
    for x in &plan.items {
        if cancel.load(Ordering::Acquire) {
            return ExecutionReport {
                generation: plan.generation,
                items: results,
                affected_subtree: smallest_common_subtree(&affected),
                cancelled: true,
            };
        }
        let validation = ensure_contained(&plan.root, &x.relative).and_then(|p| {
            if p != x.target {
                Err("captured target changed".into())
            } else if dirty.contains(&p) {
                Err("item has unsaved Diff changes".into())
            } else if FileIdentity::read(&p).map_err(|e| e.to_string())? != x.expected {
                Err("item changed since confirmation".into())
            } else {
                Ok(())
            }
        });
        let outcome = match validation {
            Err(e) => ItemOutcome::Failed(e),
            Ok(()) => {
                let r = match plan.mode {
                    DeleteMode::Recycle => backend.recycle(&x.target),
                    DeleteMode::Permanent => backend.permanently_delete(&x.target),
                };
                match r {
                    Ok(()) => {
                        affected.push(x.relative.clone());
                        ItemOutcome::Copied
                    }
                    Err(e) => ItemOutcome::Failed(e),
                }
            }
        };
        results.push(ItemResult {
            relative: x.relative.clone(),
            outcome,
        })
    }
    ExecutionReport {
        generation: plan.generation,
        items: results,
        affected_subtree: smallest_common_subtree(&affected),
        cancelled: false,
    }
}
