//! Test-only receipts for the final release of real filesystem callback
//! closures created by the Track C startup fixtures.
//!
//! This is deliberately narrower than an OS watcher join: `notify` owns a
//! detached backend thread on Windows, while these receipts prove only that
//! the callback closure itself no longer retains the fixture paths.

use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak, mpsc};
use std::time::{Duration, Instant};

const MAX_HANDLER_ATTEMPTS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) enum HandlerKind {
    JsonFolders,
    JsonBookmarks,
    JsonTodos,
    JsonDashboard,
    NotePlugin,
    GuiFolders,
    GuiBookmarks,
    GuiTodos,
    GuiNotes,
    ClipboardModify,
    GuiActions,
}

impl HandlerKind {
    pub(crate) const EXPECTED: [Self; 10] = [
        Self::JsonFolders,
        Self::JsonBookmarks,
        Self::JsonTodos,
        Self::JsonDashboard,
        Self::NotePlugin,
        Self::GuiFolders,
        Self::GuiBookmarks,
        Self::GuiTodos,
        Self::GuiNotes,
        Self::ClipboardModify,
    ];

    pub(crate) const fn token(self) -> &'static str {
        match self {
            Self::JsonFolders => "json_folders",
            Self::JsonBookmarks => "json_bookmarks",
            Self::JsonTodos => "json_todos",
            Self::JsonDashboard => "json_dashboard",
            Self::NotePlugin => "note_plugin",
            Self::GuiFolders => "gui_folders",
            Self::GuiBookmarks => "gui_bookmarks",
            Self::GuiTodos => "gui_todos",
            Self::GuiNotes => "gui_notes",
            Self::ClipboardModify => "clipboard_modify",
            Self::GuiActions => "gui_actions",
        }
    }
}

#[derive(Debug)]
struct ProbeState {
    root: PathBuf,
    next_id: AtomicU64,
    attempts: Mutex<Vec<(u64, HandlerKind)>>,
    created: Mutex<Vec<(u64, HandlerKind)>>,
    released_tx: mpsc::SyncSender<(u64, HandlerKind)>,
    overflowed: std::sync::atomic::AtomicBool,
}

static ACTIVE: Lazy<Mutex<Option<Weak<ProbeState>>>> = Lazy::new(|| Mutex::new(None));
static PROBE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// A non-cloneable token retained by the actual notify callback closure.
pub(crate) struct HandlerReleaseSentinel {
    id: u64,
    kind: HandlerKind,
    state: Arc<ProbeState>,
}

impl HandlerReleaseSentinel {
    pub(crate) fn registration(&self) -> HandlerRegistration {
        HandlerRegistration {
            id: self.id,
            kind: self.kind,
            state: Arc::clone(&self.state),
        }
    }
}

impl Drop for HandlerReleaseSentinel {
    fn drop(&mut self) {
        if self
            .state
            .released_tx
            .try_send((self.id, self.kind))
            .is_err()
        {
            self.state.overflowed.store(true, Ordering::Release);
        }
    }
}

/// Separate registration ticket lets the watcher owner attest successful
/// backend setup without cloning or moving the callback's release sentinel.
pub(crate) struct HandlerRegistration {
    id: u64,
    kind: HandlerKind,
    state: Arc<ProbeState>,
}

impl HandlerRegistration {
    pub(crate) fn registered(self) {
        self.state
            .created
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((self.id, self.kind));
    }
}

pub(crate) fn capture(kind: HandlerKind, watched_path: &Path) -> Option<HandlerReleaseSentinel> {
    let state = ACTIVE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()?
        .upgrade()?;
    let path = normalize_owned_path(watched_path);
    if !path.starts_with(&state.root) {
        return None;
    }

    let mut attempts = state
        .attempts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if attempts.len() >= MAX_HANDLER_ATTEMPTS {
        state.overflowed.store(true, Ordering::Release);
        return None;
    }
    let id = state
        .next_id
        .fetch_add(1, Ordering::Relaxed)
        .saturating_add(1);
    attempts.push((id, kind));
    drop(attempts);
    Some(HandlerReleaseSentinel { id, kind, state })
}

fn normalize_owned_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };

    let mut existing = absolute.as_path();
    let mut missing_suffix = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            break;
        };
        missing_suffix.push(name.to_os_string());
        let Some(parent) = existing.parent() else {
            break;
        };
        existing = parent;
    }

    let mut normalized = std::fs::canonicalize(existing).unwrap_or_else(|_| existing.to_path_buf());
    for component in missing_suffix.into_iter().rev() {
        normalized.push(component);
    }
    normalized
}

#[derive(Debug)]
pub(crate) struct ReleaseReport {
    pub attempts: Vec<HandlerKind>,
    pub created: Vec<HandlerKind>,
    pub released: Vec<HandlerKind>,
    pub overflowed: bool,
    pub complete: bool,
}

/// Owns the fixture paths until every captured callback has released them.
/// If a test unwinds or times out while callbacks remain live, the entire
/// workspace guard is intentionally leaked so its CWD and path overrides are
/// not restored underneath a callback that may still run.
pub(crate) struct HandlerOwnedWorkspace {
    workspace: Option<crate::performance::track_c_workloads::TrackCWorkspace>,
    callbacks_quiescent: bool,
}

impl HandlerOwnedWorkspace {
    pub(crate) fn new() -> Self {
        Self {
            workspace: Some(crate::performance::track_c_workloads::TrackCWorkspace::new()),
            callbacks_quiescent: true,
        }
    }

    pub(crate) fn root(&self) -> &Path {
        self.workspace
            .as_ref()
            .expect("handler-owned workspace remains present")
            .root()
    }

    pub(crate) fn original_cwd(&self) -> &Path {
        self.workspace
            .as_ref()
            .expect("handler-owned workspace remains present")
            .original_cwd()
    }

    pub(crate) fn original_notes_directory(&self) -> Option<std::ffi::OsString> {
        self.workspace
            .as_ref()
            .expect("handler-owned workspace remains present")
            .original_notes_directory()
    }

    pub(crate) fn original_template_directory(&self) -> Option<std::ffi::OsString> {
        self.workspace
            .as_ref()
            .expect("handler-owned workspace remains present")
            .original_template_directory()
    }

    pub(crate) fn original_skip_clipboard_sync(&self) -> Option<std::ffi::OsString> {
        self.workspace
            .as_ref()
            .expect("handler-owned workspace remains present")
            .original_skip_clipboard_sync()
    }

    pub(crate) fn callbacks_may_use_paths(&mut self) {
        self.callbacks_quiescent = false;
    }

    pub(crate) fn callbacks_released(&mut self, report: &ReleaseReport) {
        self.callbacks_quiescent = report.complete;
    }
}

impl Drop for HandlerOwnedWorkspace {
    fn drop(&mut self) {
        if !self.callbacks_quiescent
            && let Some(workspace) = self.workspace.take()
        {
            std::mem::forget(workspace);
        }
    }
}

/// Serializes the opt-in callback fixture and watches one owned root only.
pub(crate) struct HandlerReleaseProbe {
    state: Arc<ProbeState>,
    released_rx: mpsc::Receiver<(u64, HandlerKind)>,
    _serial: MutexGuard<'static, ()>,
}

impl HandlerReleaseProbe {
    pub(crate) fn begin(root: &Path) -> Self {
        let serial = PROBE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let root = normalize_owned_path(root);
        let (released_tx, released_rx) = mpsc::sync_channel(MAX_HANDLER_ATTEMPTS);
        let state = Arc::new(ProbeState {
            root,
            next_id: AtomicU64::new(0),
            attempts: Mutex::new(Vec::new()),
            created: Mutex::new(Vec::new()),
            released_tx,
            overflowed: std::sync::atomic::AtomicBool::new(false),
        });
        let mut active = ACTIVE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(active.as_ref().and_then(Weak::upgrade).is_none());
        *active = Some(Arc::downgrade(&state));
        drop(active);
        Self {
            state,
            released_rx,
            _serial: serial,
        }
    }

    pub(crate) fn wait(self, timeout: Duration) -> ReleaseReport {
        let created = self
            .state
            .created
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let attempts_with_ids = self
            .state
            .attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let attempts = attempts_with_ids
            .iter()
            .map(|(_, kind)| *kind)
            .collect::<Vec<_>>();
        let attempt_ids = attempts_with_ids
            .iter()
            .map(|(id, _)| *id)
            .collect::<HashSet<_>>();
        let deadline = Instant::now() + timeout;
        let mut released = HashSet::new();
        while released.len() < attempt_ids.len() {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            match self.released_rx.recv_timeout(remaining) {
                Ok((id, _kind)) if attempt_ids.contains(&id) => {
                    released.insert(id);
                }
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }
        let released_kinds = attempts_with_ids
            .iter()
            .filter_map(|(id, kind)| released.contains(id).then_some(*kind))
            .collect::<Vec<_>>();
        let created_kinds = created.iter().map(|(_, kind)| *kind).collect::<Vec<_>>();
        let overflowed = self.state.overflowed.load(Ordering::Acquire);
        let complete = !overflowed && released.len() == attempt_ids.len();
        ReleaseReport {
            attempts,
            created: created_kinds,
            released: released_kinds,
            overflowed,
            complete,
        }
    }
}

impl Drop for HandlerReleaseProbe {
    fn drop(&mut self) {
        let mut active = ACTIVE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if active
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|state| Arc::ptr_eq(&state, &self.state))
        {
            *active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HandlerKind, HandlerReleaseProbe, capture, normalize_owned_path};
    use crate::performance::track_c_workloads::TrackCWorkspace;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn owned_path_capture_normalizes_absolute_relative_and_missing_targets() {
        let workspace = TrackCWorkspace::new();
        let root = workspace.root();
        let probe = HandlerReleaseProbe::begin(root);
        let absolute_missing = root.join("not-created").join("folders.json");
        let relative_missing = Path::new("not-created").join("bookmarks.json");

        assert!(normalize_owned_path(&absolute_missing).starts_with(normalize_owned_path(root)));
        assert!(normalize_owned_path(&relative_missing).starts_with(normalize_owned_path(root)));
        let absolute = capture(HandlerKind::JsonFolders, &absolute_missing)
            .expect("an absolute not-yet-created target remains inside the owned root");
        let relative = capture(HandlerKind::JsonBookmarks, &relative_missing)
            .expect("a relative not-yet-created target resolves against the owned CWD");
        drop((absolute, relative));

        let report = probe.wait(Duration::from_secs(1));
        assert!(report.complete);
        assert!(!report.overflowed);
        assert_eq!(
            report.attempts,
            [HandlerKind::JsonFolders, HandlerKind::JsonBookmarks]
        );
        assert!(report.created.is_empty());
        assert_eq!(report.released, report.attempts);
    }

    #[test]
    fn failed_registration_attempt_is_not_released_until_its_handler_drops() {
        let workspace = TrackCWorkspace::new();
        let probe = HandlerReleaseProbe::begin(workspace.root());
        let sentinel = capture(
            HandlerKind::JsonDashboard,
            &workspace.root().join("dashboard.json"),
        )
        .expect("the owned dashboard callback is captured before registration");

        assert!(matches!(
            probe.released_rx.recv_timeout(Duration::from_millis(10)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        drop(sentinel);

        let report = probe.wait(Duration::from_secs(1));
        assert!(report.complete);
        assert!(!report.overflowed);
        assert_eq!(report.attempts, [HandlerKind::JsonDashboard]);
        assert!(
            report.created.is_empty(),
            "failed registration is not counted as created"
        );
        assert_eq!(report.released, [HandlerKind::JsonDashboard]);
    }
}
