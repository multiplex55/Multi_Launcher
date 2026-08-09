//! Filesystem watching and external-change arbitration for diff views.
//!
//! The types in this module deliberately do not depend on egui.  `WatchSet`
//! adapts `notify`, while `EventCoalescer` and `ExternalDocument` can be driven
//! by fake events in deterministic tests.

use crate::diff::text_file::{FileIdentity, LoadedTextFile, load_text_file};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WatchTag {
    pub workspace: u64,
    pub view: u64,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchScope {
    File,
    Root,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEvent {
    pub tag: WatchTag,
    /// The watched file, or the recursive root (never a transient spelling
    /// supplied by a backend).
    pub identity_path: PathBuf,
    pub paths: Vec<PathBuf>,
    pub scope: WatchScope,
}

/// Owns exactly one backend watcher for a view. Dropping or replacing it tears
/// down all registrations, preventing old sessions from delivering events.
pub struct WatchSet {
    watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<Event>>,
    tag: WatchTag,
    registrations: Vec<(PathBuf, WatchScope)>,
}
impl WatchSet {
    pub fn files(tag: WatchTag, paths: impl IntoIterator<Item = PathBuf>) -> notify::Result<Self> {
        Self::new(tag, paths.into_iter().map(|p| (p, WatchScope::File)))
    }
    pub fn roots(tag: WatchTag, roots: impl IntoIterator<Item = PathBuf>) -> notify::Result<Self> {
        Self::new(tag, roots.into_iter().map(|p| (p, WatchScope::Root)))
    }
    fn new(
        tag: WatchTag,
        values: impl IntoIterator<Item = (PathBuf, WatchScope)>,
    ) -> notify::Result<Self> {
        let (tx, receiver) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |e| {
            let _ = tx.send(e);
        })?;
        let mut registrations = Vec::new();
        for (path, scope) in values {
            let path = normalize_absolute(&path);
            watcher.watch(
                &path,
                if scope == WatchScope::Root {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                },
            )?;
            registrations.push((path, scope));
        }
        Ok(Self {
            watcher,
            receiver,
            tag,
            registrations,
        })
    }
    pub fn tag(&self) -> WatchTag {
        self.tag
    }
    pub fn try_events(&self) -> Vec<notify::Result<WatchEvent>> {
        self.receiver
            .try_iter()
            .map(|result| result.map(|event| self.map_event(event)))
            .collect()
    }
    fn map_event(&self, event: Event) -> WatchEvent {
        let normalized: Vec<_> = event.paths.iter().map(|p| normalize_absolute(p)).collect();
        let registration = self
            .registrations
            .iter()
            .find(|(watched, scope)| match scope {
                WatchScope::File => normalized
                    .iter()
                    .any(|p| p == watched || p.parent() == watched.parent()),
                WatchScope::Root => normalized.iter().any(|p| p.starts_with(watched)),
            })
            .or_else(|| self.registrations.first());
        let (identity_path, scope) = registration
            .cloned()
            .unwrap_or((PathBuf::new(), WatchScope::File));
        let paths = normalized
            .into_iter()
            .filter(|p| match scope {
                WatchScope::File => p == &identity_path,
                WatchScope::Root => p.starts_with(&identity_path),
            })
            .collect();
        WatchEvent {
            tag: self.tag,
            identity_path,
            paths,
            scope,
        }
    }
}
impl Drop for WatchSet {
    fn drop(&mut self) {
        for (p, _) in &self.registrations {
            let _ = self.watcher.unwatch(p);
        }
    }
}

#[derive(Debug)]
pub struct EventCoalescer {
    delay: Duration,
    pending: HashMap<(WatchTag, PathBuf), Pending>,
}
#[derive(Debug)]
struct Pending {
    deadline: Instant,
    scope: WatchScope,
    paths: BTreeSet<PathBuf>,
}
impl EventCoalescer {
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending: HashMap::new(),
        }
    }
    pub fn push(&mut self, now: Instant, event: WatchEvent) {
        let p = self
            .pending
            .entry((event.tag, event.identity_path))
            .or_insert_with(|| Pending {
                deadline: now + self.delay,
                scope: event.scope,
                paths: BTreeSet::new(),
            });
        p.deadline = now + self.delay;
        p.paths.extend(event.paths);
    }
    pub fn drain_ready(&mut self, now: Instant, current: WatchTag) -> Vec<WatchEvent> {
        let keys: Vec<_> = self
            .pending
            .iter()
            .filter(|((tag, _), p)| *tag != current || p.deadline <= now)
            .map(|(k, _)| k.clone())
            .collect();
        keys.into_iter()
            .filter_map(|key| {
                let pending = self.pending.remove(&key)?;
                if key.0 != current {
                    return None;
                }
                Some(WatchEvent {
                    tag: key.0,
                    identity_path: key.1,
                    paths: pending.paths.into_iter().collect(),
                    scope: pending.scope,
                })
            })
            .collect()
    }
    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

/// Returns the smallest relative subtree containing all affected paths.
pub fn affected_subtree(root: &Path, paths: &[PathBuf]) -> Option<PathBuf> {
    let mut rels = paths
        .iter()
        .filter_map(|p| p.strip_prefix(root).ok())
        .map(|p| {
            if p.extension().is_some() {
                p.parent().unwrap_or(Path::new(""))
            } else {
                p
            }
        })
        .map(Path::to_path_buf);
    let first = rels.next()?;
    Some(rels.fold(first, common_ancestor))
}
fn common_ancestor(a: PathBuf, b: PathBuf) -> PathBuf {
    a.components()
        .zip(b.components())
        .take_while(|(x, y)| x == y)
        .fold(PathBuf::new(), |mut p, (c, _)| {
            p.push(c.as_os_str());
            p
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalState {
    Current,
    Reloading,
    Conflict,
    Missing(String),
}
#[derive(Debug, Clone)]
pub struct ReloadTicket {
    pub revision: u64,
    pub path: PathBuf,
    pub tag: WatchTag,
}
#[derive(Debug, Clone)]
pub struct ExternalDocument {
    pub path: PathBuf,
    pub identity: Option<FileIdentity>,
    pub revision: u64,
    pub dirty: bool,
    pub state: ExternalState,
    own_save: Option<FileIdentity>,
    pub tag: WatchTag,
}
impl ExternalDocument {
    pub fn new(
        path: PathBuf,
        identity: Option<FileIdentity>,
        revision: u64,
        tag: WatchTag,
    ) -> Self {
        Self {
            path: normalize_absolute(&path),
            identity,
            revision,
            dirty: false,
            state: ExternalState::Current,
            own_save: None,
            tag,
        }
    }
    pub fn record_save(&mut self, identity: FileIdentity) {
        self.identity = Some(identity.clone());
        self.own_save = Some(identity);
        self.dirty = false;
        self.state = ExternalState::Current;
    }
    /// Re-stats after every event. Only the exact identity produced by our save
    /// is suppressed; elapsed time is intentionally irrelevant.
    pub fn observe(&mut self) -> Option<ReloadTicket> {
        match stat_identity(&self.path) {
            Ok(now) if self.own_save.as_ref() == Some(&now) => {
                self.own_save = None;
                self.identity = Some(now);
                None
            }
            Ok(now) if self.identity.as_ref() == Some(&now) => None,
            Ok(_) if self.dirty => {
                self.state = ExternalState::Conflict;
                None
            }
            Ok(_) => {
                self.state = ExternalState::Reloading;
                Some(self.ticket())
            }
            Err(e) => {
                self.state = ExternalState::Missing(e.to_string());
                None
            }
        }
    }
    pub fn ticket(&self) -> ReloadTicket {
        ReloadTicket {
            revision: self.revision,
            path: self.path.clone(),
            tag: self.tag,
        }
    }
    pub fn accept_reload(&mut self, ticket: &ReloadTicket, loaded: &LoadedTextFile) -> bool {
        if ticket.revision != self.revision
            || ticket.path != self.path
            || ticket.tag != self.tag
            || self.dirty
        {
            return false;
        }
        self.identity = Some(loaded.identity.clone());
        self.state = ExternalState::Current;
        true
    }
    pub fn keep_buffer(&mut self) {
        self.state = ExternalState::Conflict;
    }
    pub fn change_context(&mut self, path: PathBuf, tag: WatchTag) {
        self.path = normalize_absolute(&path);
        self.tag = tag;
        self.own_save = None;
        self.state = ExternalState::Current;
    }
    pub fn load(ticket: &ReloadTicket) -> io::Result<LoadedTextFile> {
        load_text_file(&ticket.path)
    }
}
pub fn stat_identity(path: &Path) -> io::Result<FileIdentity> {
    let m = fs::metadata(path)?;
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Ok(FileIdentity {
        size: m.len(),
        modified: m.modified().ok(),
        #[cfg(unix)]
        device: m.dev(),
        #[cfg(unix)]
        inode: m.ino(),
    })
}
fn normalize_absolute(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut out = PathBuf::new();
    for c in absolute.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(c.as_os_str()),
        }
    }
    out
}
