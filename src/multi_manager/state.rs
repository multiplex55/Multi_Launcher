use crate::multi_manager::model::{MmWorkspace, PendingCaptureAction, RecaptureQueueItem};
use crate::multi_manager::runtime::MultiManagerRuntime;
use crate::multi_manager::{reconnect, store};
use crate::settings::MultiManagerSettings;
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const AUTO_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

pub struct MultiManagerState {
    pub dirty: bool,
    pub pending_capture: Option<PendingCaptureAction>,
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
        let mut loaded = store::load_or_default(&workspace_path);
        if settings.auto_reconnect_on_load {
            reconnect::reconnect_workspaces(&mut loaded);
        }
        let workspaces = Arc::new(Mutex::new(loaded));
        let runtime = if settings.enabled {
            MultiManagerRuntime::start(Arc::clone(&workspaces), settings.clone())
        } else {
            MultiManagerRuntime::inactive(Arc::clone(&workspaces))
        };
        let last_hotkey_info = Arc::clone(&runtime.last_hotkey_info);

        Self {
            dirty: false,
            pending_capture: None,
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
        if self.auto_reconnect_on_load {
            reconnect::reconnect_workspaces(&mut loaded);
        }
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
        {
            if let Err(err) = self.save() {
                tracing::error!(error = %err, "failed to auto-save MultiManager workspaces");
                self.last_save_attempt = Some(Instant::now());
            }
        }
    }

    pub fn shutdown(&mut self) {
        self.capture_session = None;
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
