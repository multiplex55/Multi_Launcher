use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::tempfile::{
    clear_files, create_file, create_named_file, list_files, remove_file, set_alias, TempfilePlugin,
};
use multi_launcher::{actions::Action, launcher::launch_action};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn search_tmp_returns_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TempfilePlugin;
    let results = plugin.search("tmp");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "tempfile:dialog");
}

#[test]
fn search_new_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TempfilePlugin;
    let results = plugin.search("tmp new");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "tempfile:new");
}

#[test]
fn search_new_with_name_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TempfilePlugin;
    let results = plugin.search("tmp new testfile");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "tempfile:new:testfile");
}

#[test]
fn search_open_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TempfilePlugin;
    let results = plugin.search("tmp open");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "tempfile:open");
}

#[test]
fn search_clear_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TempfilePlugin;
    let results = plugin.search("tmp clear");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "tempfile:clear");
}

#[test]
fn list_returns_existing_files() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let _ = create_file();
    let _ = create_file();

    let plugin = TempfilePlugin;
    let results = plugin.search("tmp list");
    let files = list_files().unwrap();
    assert_eq!(results.len(), files.len());
    assert!(results.iter().all(|a| a.args.is_none()));

    clear_files().unwrap();
}

#[test]
fn rm_lists_files_for_deletion() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let file = create_file().unwrap();
    let plugin = TempfilePlugin;
    let results = plugin.search("tmp rm");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("tempfile:remove:"));
    remove_file(&file).unwrap();
}

#[test]
fn launch_action_remove_deletes_file() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let file = create_file().unwrap();
    let action = Action {
        label: "".into(),
        desc: "".into(),
        action: format!("tempfile:remove:{}", file.to_string_lossy()),
        args: None,
    };
    launch_action(&action).unwrap();
    assert!(!file.exists());
}

#[test]
fn set_alias_renames_file() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let file = create_file().unwrap();
    let new = set_alias(&file, "alias").unwrap();
    assert!(new
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("temp_alias"));
    remove_file(&new).unwrap();
}

#[test]
fn create_named_file_rejects_invalid_alias() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let res = create_named_file("bad/alias", "hi");
    assert!(res.is_err());
}

#[test]
fn set_alias_rejects_invalid_alias() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let file = create_file().unwrap();
    let res = set_alias(&file, "bad/alias");
    assert!(res.is_err());
    remove_file(&file).unwrap();
}
