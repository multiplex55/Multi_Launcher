use crate::multi_manager::model::{MmWorkspace, PendingCaptureAction, RecaptureQueueItem};
use crate::multi_manager::runtime::MultiManagerRuntime;
use crate::multi_manager::{bindings, reconnect, store, win};
use crate::settings::MultiManagerSettings;
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const AUTO_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

pub struct MultiManagerState {
    pub dirty: bool,
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
    pub auto_reconnect_missing_windows: bool,
    pub reconnect_interval: Duration,
    save_debounce: Duration,
    dirty_since: Option<Instant>,
    last_save_attempt: Option<Instant>,
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
        // `auto_reconnect_missing_windows` controls the runtime's periodic reconnect loop.
        let loaded = prepare_workspaces_for_startup(
            store::load_or_default(&workspace_path),
            &bindings_path,
            settings.auto_reconnect_on_load,
        );
        let workspaces = Arc::new(Mutex::new(loaded));
        let runtime = start_runtime_after_restore(Arc::clone(&workspaces), settings);
        let last_hotkey_info = Arc::clone(&runtime.last_hotkey_info);

        Self {
            dirty: false,
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
            auto_reconnect_missing_windows: settings.auto_reconnect_missing_windows,
            reconnect_interval: Duration::from_millis(settings.auto_reconnect_interval_ms),
            save_debounce: AUTO_SAVE_DEBOUNCE,
            dirty_since: None,
            last_save_attempt: None,
        }
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

    pub fn reload(&mut self) -> Result<()> {
        let mut loaded = store::load_workspaces(&self.workspace_path)?;
        restore_and_optionally_reconnect(
            &mut loaded,
            &self.bindings_path,
            self.auto_reconnect_on_load,
        );
        let mut workspaces = self
            .workspaces
            .lock()
            .map_err(|_| anyhow::anyhow!("MultiManager workspace lock poisoned"))?;
        *workspaces = loaded;
        self.dirty = false;
        self.dirty_since = None;
        Ok(())
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
    }

    pub fn save_debounced(&mut self) {
        self.mark_dirty();
        self.maybe_auto_save();
    }

    pub fn maybe_auto_save(&mut self) {
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
        self.capture_session = None;
        self.pending_capture = None;
        self.queued_capture = None;
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
    auto_reconnect_on_load: bool,
) -> Vec<MmWorkspace> {
    restore_and_optionally_reconnect(&mut workspaces, bindings_path, auto_reconnect_on_load);
    workspaces
}

fn restore_and_optionally_reconnect(
    workspaces: &mut [MmWorkspace],
    bindings_path: &Path,
    auto_reconnect_on_load: bool,
) {
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

    if snapshots_loaded && auto_reconnect_on_load {
        let live = win::enumerate_top_level_windows().unwrap_or_default();
        reconnect::reconnect_unresolved_workspaces_with_windows(workspaces, &live);
    }
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
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state.shutdown();
        state.shutdown();
    }
}
