use multi_launcher::plugins::folders::{append_folder, load_folders, save_folders, FOLDERS_FILE};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn adding_nonexistent_folder_returns_error() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_folders(FOLDERS_FILE, &[]).unwrap();
    let missing = dir.path().join("does_not_exist");
    let res = append_folder(FOLDERS_FILE, missing.to_str().unwrap());
    assert!(res.is_err());

    let list = load_folders(FOLDERS_FILE).unwrap();
    assert!(list.is_empty());
}
