use crate::multi_manager::model::{MmWorkspace, PendingCaptureAction, RecaptureQueueItem};
use crate::multi_manager::runtime::{MultiManagerRuntime, MultiManagerRuntimeEvent};
use crate::multi_manager::{bindings, reconnect, store, win};
use crate::settings::MultiManagerSettings;
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const AUTO_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
const BINDINGS_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
pub const LIVE_TITLE_REFRESH_INTERVAL: Duration = Duration::from_millis(750);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectTrigger {
    Startup,
    Reload,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectStartResult {
    Started,
    AlreadyRunning,
    SnapshotLockFailed,
}

struct ReconnectInProgressGuard {
    flag: Arc<AtomicBool>,
}

impl Drop for ReconnectInProgressGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

pub struct MultiManagerState {
    pub dirty: bool,
    pub bindings_dirty: bool,
    pub pending_capture: Option<PendingCaptureAction>,
    pub queued_capture: Option<PendingCaptureAction>,
    pub recapture_queue: VecDeque<RecaptureQueueItem>,
    pub recapture_active: bool,
    pub capture_session: Option<crate::multi_manager::capture::CaptureSession>,
    pub workspaces: Arc<Mutex<Vec<MmWorkspace>>>,
    pub runtime: MultiManagerRuntime,
    pub last_hotkey_info: Arc<Mutex<Option<(String, Instant)>>>,
    pub workspace_path: PathBuf,
    pub bindings_path: PathBuf,
    pub auto_save: bool,
    pub auto_reconnect_on_load: bool,
    save_debounce: Duration,
    dirty_since: Option<Instant>,
    last_save_attempt: Option<Instant>,
    binding_save_debounce: Duration,
    pub bindings_dirty_since: Option<Instant>,
    pub last_bindings_save_attempt: Option<Instant>,
    pub last_live_title_refresh: Option<Instant>,
    pub reconnect_in_progress: Arc<AtomicBool>,
    pub reconnect_job: Option<JoinHandle<()>>,
    pub pending_automatic_reconnect: bool,
    pub shutdown_started: bool,
}

impl MultiManagerState {
    pub fn load_or_default(settings: &MultiManagerSettings, settings_path: &str) -> Self {
        let settings_dir = Path::new(settings_path)
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let workspace_path = resolve_relative_to(settings_dir, &settings.workspaces_path);
        let bindings_path = resolve_relative_to(settings_dir, &settings.bindings_path);

        // Startup always attempts to restore saved HWND binding snapshots first.
        // `auto_reconnect_on_load` only controls the exact-title fallback for windows
        // that snapshot restore could not resolve during startup/reload.
        let loaded = prepare_workspaces_for_startup(
            store::load_or_default(&workspace_path),
            &bindings_path,
            settings.auto_reconnect_on_load,
        );
        let workspaces = Arc::new(Mutex::new(loaded));
        let runtime = start_runtime_after_restore(Arc::clone(&workspaces), settings);
        let last_hotkey_info = Arc::clone(&runtime.last_hotkey_info);

        let mut state = Self {
            dirty: false,
            bindings_dirty: false,
            pending_capture: None,
            queued_capture: None,
            recapture_queue: VecDeque::new(),
            recapture_active: false,
            capture_session: None,
            workspaces,
            runtime,
            last_hotkey_info,
            workspace_path,
            bindings_path,
            auto_save: settings.auto_save,
            auto_reconnect_on_load: settings.auto_reconnect_on_load,
            save_debounce: AUTO_SAVE_DEBOUNCE,
            dirty_since: None,
            last_save_attempt: None,
            binding_save_debounce: BINDINGS_SAVE_DEBOUNCE,
            bindings_dirty_since: None,
            last_bindings_save_attempt: None,
            last_live_title_refresh: None,
            reconnect_in_progress: Arc::new(AtomicBool::new(false)),
            reconnect_job: None,
            pending_automatic_reconnect: false,
            shutdown_started: false,
        };
        if settings.auto_reconnect_on_load {
            let _ = state.start_reconnect(ReconnectTrigger::Startup);
        }
        state
    }

    pub fn save(&mut self) -> Result<()> {
        let workspaces = self
            .workspaces
            .lock()
            .map_err(|_| anyhow::anyhow!("MultiManager workspace lock poisoned"))?;
        store::save_workspaces(&self.workspace_path, &workspaces).with_context(|| {
            format!(
                "failed to save MultiManager workspaces to {}",
                self.workspace_path.display()
            )
        })?;
        self.dirty = false;
        self.dirty_since = None;
        self.last_save_attempt = Some(Instant::now());
        Ok(())
    }

    pub fn save_bindings_now(&mut self) -> Result<()> {
        let workspaces = self
            .workspaces
            .lock()
            .map_err(|_| anyhow::anyhow!("MultiManager workspace lock poisoned"))?;
        bindings::save_bindings(&self.bindings_path, &workspaces).with_context(|| {
            format!(
                "failed to save MultiManager bindings to {}",
                self.bindings_path.display()
            )
        })?;
        self.bindings_dirty = false;
        self.bindings_dirty_since = None;
        self.runtime
            .control
            .bindings_dirty_signal
            .store(false, Ordering::Relaxed);
        self.last_bindings_save_attempt = Some(Instant::now());
        Ok(())
    }

    pub fn reload(&mut self) -> Result<()> {
        let mut loaded = store::load_workspaces(&self.workspace_path)?;
        restore_bindings_for_load(&mut loaded, &self.bindings_path);
        {
            let mut workspaces = self
                .workspaces
                .lock()
                .map_err(|_| anyhow::anyhow!("MultiManager workspace lock poisoned"))?;
            *workspaces = loaded;
        }
        self.dirty = false;
        self.dirty_since = None;
        self.bindings_dirty = false;
        self.bindings_dirty_since = None;
        self.runtime
            .control
            .bindings_dirty_signal
            .store(false, Ordering::Relaxed);
        if self.auto_reconnect_on_load {
            let _ = self.start_reconnect(ReconnectTrigger::Reload);
        }
        Ok(())
    }

    pub fn start_reconnect(&mut self, trigger: ReconnectTrigger) -> ReconnectStartResult {
        self.reap_reconnect_worker();
        if self.shutdown_started || self.runtime.control.shutdown.load(Ordering::Acquire) {
            return ReconnectStartResult::AlreadyRunning;
        }
        if self
            .reconnect_in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            if trigger != ReconnectTrigger::Manual {
                self.pending_automatic_reconnect = true;
            }
            return ReconnectStartResult::AlreadyRunning;
        }
        match self.workspaces.try_lock() {
            Ok(_) => {}
            Err(TryLockError::Poisoned(_)) | Err(TryLockError::WouldBlock) => {
                self.reconnect_in_progress.store(false, Ordering::Release);
                return ReconnectStartResult::SnapshotLockFailed;
            }
        }
        let workspaces = Arc::clone(&self.workspaces);
        let events = Arc::clone(&self.runtime.event_queue);
        let bindings_dirty = Arc::clone(&self.runtime.control.bindings_dirty_signal);
        let in_progress = Arc::clone(&self.reconnect_in_progress);
        self.reconnect_job = Some(thread::spawn(move || {
            let _guard = ReconnectInProgressGuard { flag: in_progress };
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                let snapshot = {
                    let guard = workspaces.lock().map_err(|_| {
                        "workspace lock poisoned during reconnect snapshot".to_string()
                    })?;
                    reconnect::collect_reconnect_snapshot(&guard)
                };
                let is_window = |hwnd| win::is_valid_window(hwnd);
                let query_identity = |hwnd| Some(win::query_hwnd_identity(hwnd));
                let enumerate = || win::enumerate_top_level_windows().unwrap_or_default();
                let (_worker_summary, patches) = reconnect::build_reconnect_patches(
                    &snapshot,
                    reconnect::ReconnectDeps {
                        is_window: &is_window,
                        query_identity: &query_identity,
                        enumerate_top_level_windows: &enumerate,
                    },
                );
                let mut guard = workspaces
                    .lock()
                    .map_err(|_| "workspace lock poisoned during reconnect apply".to_string())?;
                let summary = reconnect::apply_reconnect_patches(&mut guard, &patches);
                if summary.binding_snapshot_changed {
                    bindings_dirty.store(true, Ordering::Relaxed);
                }
                Ok::<_, String>(summary)
            }));
            let event = match result {
                Ok(Ok(summary)) => {
                    MultiManagerRuntimeEvent::ReconnectCompleted { trigger, summary }
                }
                Ok(Err(error)) => MultiManagerRuntimeEvent::ReconnectFailed { trigger, error },
                Err(_) => MultiManagerRuntimeEvent::ReconnectFailed {
                    trigger,
                    error: "reconnect worker panicked".to_string(),
                },
            };
            if let Ok(mut queue) = events.lock() {
                queue.push_back(event);
            }
        }));
        ReconnectStartResult::Started
    }

    pub fn reap_reconnect_worker(&mut self) {
        if self
            .reconnect_job
            .as_ref()
            .is_some_and(|job| job.is_finished())
        {
            if let Some(job) = self.reconnect_job.take() {
                let _ = job.join();
            }
            if self.pending_automatic_reconnect && !self.shutdown_started {
                self.pending_automatic_reconnect = false;
                let _ = self.start_reconnect(ReconnectTrigger::Reload);
            }
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
    }

    pub fn mark_bindings_dirty(&mut self) {
        self.bindings_dirty = true;
        if self.bindings_dirty_since.is_none() {
            self.bindings_dirty_since = Some(Instant::now());
        }
    }

    pub fn save_debounced(&mut self) {
        self.mark_dirty();
        self.maybe_auto_save();
    }

    pub fn maybe_auto_save(&mut self) {
        if self
            .runtime
            .control
            .bindings_dirty_signal
            .swap(false, Ordering::Relaxed)
        {
            self.mark_bindings_dirty();
        }
        if !self.auto_save || !self.dirty {
            return;
        }
        if self
            .dirty_since
            .is_some_and(|dirty_since| dirty_since.elapsed() >= self.save_debounce)
            && let Err(err) = self.save()
        {
            tracing::error!(error = %err, "failed to auto-save MultiManager workspaces");
            self.last_save_attempt = Some(Instant::now());
        }
    }

    pub fn maybe_auto_save_bindings(&mut self) {
        if self
            .runtime
            .control
            .bindings_dirty_signal
            .swap(false, Ordering::Relaxed)
        {
            self.mark_bindings_dirty();
        }
        if !self.auto_save || !self.bindings_dirty {
            return;
        }
        if self
            .bindings_dirty_since
            .is_some_and(|dirty_since| dirty_since.elapsed() >= self.binding_save_debounce)
            && let Err(err) = self.save_bindings_now()
        {
            tracing::error!(error = %err, "failed to auto-save MultiManager bindings");
            self.last_bindings_save_attempt = Some(Instant::now());
        }
    }

    pub fn flush_bindings_if_dirty(&mut self) -> Result<()> {
        if self
            .runtime
            .control
            .bindings_dirty_signal
            .swap(false, Ordering::Relaxed)
        {
            self.mark_bindings_dirty();
        }
        if self.bindings_dirty {
            self.save_bindings_now()?;
        }
        Ok(())
    }

    pub fn validate_capture_state_debug(&self) {
        if let (Some(pending), Some(queued)) =
            (self.pending_capture.as_ref(), self.queued_capture.as_ref())
        {
            debug_assert_eq!(
                capture_action_target(pending),
                capture_action_target(queued),
                "queued capture must not coexist with unrelated active pending_capture"
            );
        }

        debug_assert!(
            capture_state_invariant_violations(
                self.runtime.control.capture_pending.load(Ordering::Relaxed),
                self.capture_session.is_some(),
                self.pending_capture.as_ref(),
                self.queued_capture.as_ref(),
            )
            .is_empty(),
            "invalid MultiManager capture state: {:?}",
            capture_state_invariant_violations(
                self.runtime.control.capture_pending.load(Ordering::Relaxed),
                self.capture_session.is_some(),
                self.pending_capture.as_ref(),
                self.queued_capture.as_ref(),
            )
        );
    }

    pub fn shutdown(&mut self) {
        self.shutdown_started = true;
        self.pending_automatic_reconnect = false;
        self.capture_session = None;
        self.pending_capture = None;
        self.queued_capture = None;
        if let Some(job) = self.reconnect_job.take() {
            let _ = job.join();
        }
        self.runtime.shutdown();
    }

    pub fn with_workspace_mut<R>(
        &mut self,
        id: &str,
        f: impl FnOnce(&mut MmWorkspace) -> R,
    ) -> Option<R> {
        let result = {
            let mut workspaces = self.workspaces.lock().ok()?;
            let workspace = workspaces.iter_mut().find(|workspace| workspace.id == id)?;
            f(workspace)
        };
        self.mark_dirty();
        Some(result)
    }

    #[cfg(test)]
    fn force_debounce_elapsed(&mut self) {
        self.dirty_since = Some(Instant::now() - self.save_debounce - Duration::from_millis(1));
        self.bindings_dirty_since =
            Some(Instant::now() - self.binding_save_debounce - Duration::from_millis(1));
    }
}

pub(crate) fn capture_state_invariant_violations(
    capture_pending: bool,
    capture_session_active: bool,
    pending_capture: Option<&PendingCaptureAction>,
    queued_capture: Option<&PendingCaptureAction>,
) -> Vec<&'static str> {
    let has_active_or_queued_listener = capture_session_active || queued_capture.is_some();
    let mut violations = Vec::new();

    if capture_pending && !has_active_or_queued_listener {
        violations.push("capture_pending requires an active capture session or queued capture");
    }

    if pending_capture.is_some() && !has_active_or_queued_listener {
        violations.push("pending_capture requires an active capture session or queued capture");
    }

    if let (Some(pending), Some(queued)) = (pending_capture, queued_capture)
        && capture_action_target(pending) != capture_action_target(queued)
    {
        violations.push("queued_capture must not coexist with unrelated active pending_capture");
    }

    violations
}

fn capture_action_target(action: &PendingCaptureAction) -> (&str, Option<usize>) {
    match action {
        PendingCaptureAction::CaptureOneWindow { workspace_id }
        | PendingCaptureAction::CaptureMultipleWindows { workspace_id } => (workspace_id, None),
        PendingCaptureAction::RecaptureWindow {
            workspace_id,
            window_index,
        } => (workspace_id, Some(*window_index)),
    }
}

fn prepare_workspaces_for_startup(
    mut workspaces: Vec<MmWorkspace>,
    bindings_path: &Path,
    _auto_reconnect_on_load: bool,
) -> Vec<MmWorkspace> {
    restore_bindings_for_load(&mut workspaces, bindings_path);
    workspaces
}

fn restore_bindings_for_load(workspaces: &mut [MmWorkspace], bindings_path: &Path) {
    let snapshots_loaded = match bindings::load_bindings_if_exists(bindings_path) {
        Ok(Some(snapshots)) => {
            bindings::restore_bindings(workspaces, &snapshots);
            true
        }
        Ok(None) => {
            tracing::debug!(path = %bindings_path.display(), "MultiManager bindings file not found; leaving startup windows unresolved");
            false
        }
        Err(err) => {
            tracing::error!(error = %err, path = %bindings_path.display(), "failed to load MultiManager bindings; continuing without saved HWND restore");
            false
        }
    };

    let _ = snapshots_loaded;
}

fn start_runtime_after_restore(
    workspaces: Arc<Mutex<Vec<MmWorkspace>>>,
    settings: &MultiManagerSettings,
) -> MultiManagerRuntime {
    if settings.enabled {
        MultiManagerRuntime::start(workspaces, settings.clone())
    } else {
        MultiManagerRuntime::inactive(workspaces)
    }
}

fn resolve_relative_to(base: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_manager::model::MmWorkspace;

    fn capture_one(workspace_id: &str) -> PendingCaptureAction {
        PendingCaptureAction::CaptureOneWindow {
            workspace_id: workspace_id.into(),
        }
    }

    #[test]
    fn capture_state_invariants_accept_active_session_with_pending_capture() {
        let pending = capture_one("w");

        let violations = capture_state_invariant_violations(true, true, Some(&pending), None);

        assert!(violations.is_empty());
    }

    #[test]
    fn capture_state_invariants_accept_queued_capture_without_session() {
        let queued = capture_one("w");

        let violations = capture_state_invariant_violations(true, false, None, Some(&queued));

        assert!(violations.is_empty());
    }

    #[test]
    fn capture_state_invariants_reject_pending_without_active_or_queued_listener() {
        let pending = capture_one("w");

        let violations = capture_state_invariant_violations(false, false, Some(&pending), None);

        assert_eq!(
            violations,
            vec!["pending_capture requires an active capture session or queued capture"]
        );
    }

    #[test]
    fn capture_state_invariants_reject_capture_pending_without_listener() {
        let violations = capture_state_invariant_violations(true, false, None, None);

        assert_eq!(
            violations,
            vec!["capture_pending requires an active capture session or queued capture"]
        );
    }

    #[test]
    fn capture_state_invariants_reject_unrelated_pending_and_queued_capture() {
        let pending = capture_one("active");
        let queued = capture_one("queued");

        let violations =
            capture_state_invariant_violations(true, true, Some(&pending), Some(&queued));

        assert_eq!(
            violations,
            vec!["queued_capture must not coexist with unrelated active pending_capture"]
        );
    }

    #[test]
    fn default_state_resolves_paths() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        assert_eq!(
            state.workspace_path,
            dir.path().join("multi_manager_workspaces.json")
        );
        assert_eq!(
            state.bindings_path,
            dir.path().join("multi_manager_bindings.json")
        );
    }

    #[test]
    fn missing_binding_file_does_not_break_load() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let workspace_path = dir.path().join("workspaces.json");
        std::fs::write(&workspace_path, r#"[{"id":"ws","name":"Loaded"}]"#).unwrap();

        let state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                workspaces_path: "workspaces.json".into(),
                bindings_path: "missing-bindings.json".into(),
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );

        assert_eq!(state.workspaces.lock().unwrap()[0].name, "Loaded");
        assert_eq!(
            state.bindings_path,
            dir.path().join("missing-bindings.json")
        );
    }

    #[test]
    fn malformed_binding_file_does_not_break_load() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        std::fs::write(
            dir.path().join("workspaces.json"),
            r#"[{"id":"ws","name":"Loaded"}]"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("bindings.json"), "not json").unwrap();

        let state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                workspaces_path: "workspaces.json".into(),
                bindings_path: "bindings.json".into(),
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );

        assert_eq!(state.workspaces.lock().unwrap()[0].name, "Loaded");
    }

    #[test]
    fn runtime_is_created_from_prepared_workspace_list() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        std::fs::write(
            dir.path().join("workspaces.json"),
            r#"[{"id":"","name":"Normalized"}]"#,
        )
        .unwrap();

        let state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                workspaces_path: "workspaces.json".into(),
                bindings_path: "missing-bindings.json".into(),
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );

        let runtime_workspaces = state.runtime.workspaces.lock().unwrap();
        assert_eq!(runtime_workspaces[0].name, "Normalized");
        assert!(!runtime_workspaces[0].id.is_empty());
    }

    #[test]
    fn reload_prepares_temporary_workspace_list_before_replacing_shared_list() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let workspace_path = dir.path().join("workspaces.json");
        std::fs::write(&workspace_path, r#"[{"id":"old","name":"Old"}]"#).unwrap();
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                workspaces_path: "workspaces.json".into(),
                bindings_path: "missing-bindings.json".into(),
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        std::fs::write(&workspace_path, r#"[{"id":"","name":"Reloaded"}]"#).unwrap();

        state.reload().unwrap();

        let workspaces = state.workspaces.lock().unwrap();
        assert_eq!(workspaces[0].name, "Reloaded");
        assert!(!workspaces[0].id.is_empty());
    }

    #[test]
    fn dirty_flag_changes_after_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state.workspaces.lock().unwrap().push(MmWorkspace {
            id: "a".into(),
            ..Default::default()
        });
        state.with_workspace_mut("a", |workspace| workspace.name = "changed".into());
        assert!(state.dirty);
    }

    #[test]
    fn autosave_clears_dirty_after_successful_save() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_save: true,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state.workspaces.lock().unwrap().push(MmWorkspace {
            id: "a".into(),
            ..Default::default()
        });
        state.mark_dirty();
        state.force_debounce_elapsed();
        state.maybe_auto_save();
        assert!(!state.dirty);
        assert!(state.workspace_path.exists());
    }

    #[test]
    fn save_on_exit_path_can_be_called_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state.shutdown();
        state.save().unwrap();
    }

    #[test]
    fn runtime_shutdown_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state.shutdown();
        state.shutdown();
    }
    #[test]
    fn capture_marks_bindings_dirty_without_workspace_save() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state.mark_bindings_dirty();
        assert!(state.bindings_dirty);
        assert!(!state.dirty);
    }

    #[test]
    fn fallback_reconnect_signal_marks_bindings_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state
            .runtime
            .control
            .bindings_dirty_signal
            .store(true, Ordering::Relaxed);
        state.maybe_auto_save_bindings();
        assert!(state.bindings_dirty);
        assert!(!state.dirty);
    }

    #[test]
    fn only_one_reconnect_job_can_run() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state.reconnect_in_progress.store(true, Ordering::Relaxed);

        assert_eq!(
            state.start_reconnect(ReconnectTrigger::Manual),
            ReconnectStartResult::AlreadyRunning
        );
        assert!(state.reconnect_job.is_none());
    }

    #[test]
    fn concurrent_automatic_reconnect_keeps_one_pending_request() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state.reconnect_in_progress.store(true, Ordering::Relaxed);

        assert_eq!(
            state.start_reconnect(ReconnectTrigger::Reload),
            ReconnectStartResult::AlreadyRunning
        );
        assert!(state.pending_automatic_reconnect);
        assert_eq!(
            state.start_reconnect(ReconnectTrigger::Startup),
            ReconnectStartResult::AlreadyRunning
        );
        assert!(state.pending_automatic_reconnect);
    }

    #[test]
    fn failed_manual_reconnect_can_be_retried_successfully() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        let workspaces = Arc::clone(&state.workspaces);
        let held = workspaces.lock().unwrap();
        assert_eq!(
            state.start_reconnect(ReconnectTrigger::Manual),
            ReconnectStartResult::SnapshotLockFailed
        );
        drop(held);

        assert_eq!(
            state.start_reconnect(ReconnectTrigger::Manual),
            ReconnectStartResult::Started
        );
        state.shutdown();
    }

    #[test]
    fn in_progress_flag_resets_on_snapshot_error() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        let workspaces = Arc::clone(&state.workspaces);
        let held = workspaces.lock().unwrap();
        assert_eq!(
            state.start_reconnect(ReconnectTrigger::Manual),
            ReconnectStartResult::SnapshotLockFailed
        );
        assert!(!state.reconnect_in_progress.load(Ordering::Relaxed));
        drop(held);
    }

    #[test]
    fn reconnect_worker_is_joined_by_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        assert_eq!(
            state.start_reconnect(ReconnectTrigger::Manual),
            ReconnectStartResult::Started
        );
        state.shutdown();
        assert!(state.reconnect_job.is_none());
        assert!(!state.reconnect_in_progress.load(Ordering::Relaxed));
    }

    #[test]
    fn no_new_reconnect_starts_after_shutdown_begins() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state.shutdown_started = true;

        assert_eq!(
            state.start_reconnect(ReconnectTrigger::Manual),
            ReconnectStartResult::AlreadyRunning
        );
        assert!(state.reconnect_job.is_none());
    }

    #[test]
    fn live_title_refresh_does_not_mark_bindings_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_reconnect_on_load: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        assert!(!state.bindings_dirty);
    }

    #[test]
    fn rapid_binding_changes_debounce_to_one_save_opportunity() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let mut state = MultiManagerState::load_or_default(
            &MultiManagerSettings {
                enabled: false,
                auto_save: true,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state.mark_bindings_dirty();
        let first_dirty_since = state.bindings_dirty_since;
        state.mark_bindings_dirty();
        assert_eq!(state.bindings_dirty_since, first_dirty_since);
        state.maybe_auto_save_bindings();
        assert!(state.last_bindings_save_attempt.is_none());
    }
}
