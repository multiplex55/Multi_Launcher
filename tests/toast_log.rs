use multi_launcher::gui::ToastLogDialog;
use multi_launcher::toast_log::TOAST_LOG_FILE;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn open_action_missing_file_does_not_panic() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    assert!(std::panic::catch_unwind(|| {
        if std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(TOAST_LOG_FILE)
            .is_ok()
        {
            let _ = open::that(TOAST_LOG_FILE);
        }
    })
    .is_ok());
}

#[test]
fn toast_log_dialog_open_missing_file_does_not_panic() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let mut dialog = ToastLogDialog::default();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dialog.open())).is_ok());
}
