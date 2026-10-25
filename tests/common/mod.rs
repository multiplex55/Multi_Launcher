use once_cell::sync::Lazy;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tempfile::TempDir;

static TEST_WORKSPACE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// Gives integration tests an isolated CWD and note paths, then restores them
/// after later-declared app values have been dropped.
pub(crate) struct ScopedWorkspace {
    temp: Option<TempDir>,
    previous_cwd: PathBuf,
    previous_notes_dir: Option<OsString>,
    previous_template_dir: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl ScopedWorkspace {
    pub(crate) fn new() -> Self {
        let lock = TEST_WORKSPACE_LOCK
            .lock()
            .expect("integration test workspace lock poisoned");
        let temp = tempfile::tempdir().expect("create integration test workspace");
        let root = temp.path().to_path_buf();
        let notes_dir = root.join("notes");
        let templates_dir = root.join("templates");
        std::fs::create_dir_all(&notes_dir).expect("create integration test notes directory");
        std::fs::create_dir_all(&templates_dir)
            .expect("create integration test note templates directory");
        let previous_cwd = std::env::current_dir().expect("read current directory");
        let previous_notes_dir = std::env::var_os("ML_NOTES_DIR");
        let previous_template_dir = std::env::var_os("ML_NOTE_TEMPLATES_DIR");
        std::env::set_current_dir(&root).expect("enter integration test workspace");
        // These tests run in a process-local environment; restore both variables
        // exactly, including the absent state, when the scope ends.
        unsafe {
            std::env::set_var("ML_NOTES_DIR", &notes_dir);
            std::env::set_var("ML_NOTE_TEMPLATES_DIR", &templates_dir);
        }
        Self {
            temp: Some(temp),
            previous_cwd,
            previous_notes_dir,
            previous_template_dir,
            _lock: lock,
        }
    }

}

impl Drop for ScopedWorkspace {
    fn drop(&mut self) {
        restore_env("ML_NOTES_DIR", self.previous_notes_dir.take());
        restore_env("ML_NOTE_TEMPLATES_DIR", self.previous_template_dir.take());
        let _ = std::env::set_current_dir(&self.previous_cwd);
        drop(self.temp.take());
        // `_lock` stays held until the CWD, environment, and temporary root are restored.
    }
}

fn restore_env(name: &str, previous: Option<OsString>) {
    unsafe {
        if let Some(previous) = previous {
            std::env::set_var(name, previous);
        } else {
            std::env::remove_var(name);
        }
    }
}
