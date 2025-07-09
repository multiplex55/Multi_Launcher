use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::shell::{save_shell_cmds, load_shell_cmds, ShellCmdEntry, ShellPlugin, SHELL_CMDS_FILE};
use tempfile::tempdir;
use once_cell::sync::Lazy;
use std::sync::Mutex;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn load_shell_cmds_roundtrip() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![ShellCmdEntry { name: "test".into(), args: "echo hi".into() }];
    save_shell_cmds(SHELL_CMDS_FILE, &entries).unwrap();
    let loaded = load_shell_cmds(SHELL_CMDS_FILE).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "test");
    assert_eq!(loaded[0].args, "echo hi");
}

#[test]
fn search_named_command_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![ShellCmdEntry { name: "demo".into(), args: "dir".into() }];
    save_shell_cmds(SHELL_CMDS_FILE, &entries).unwrap();

    let plugin = ShellPlugin;
    let results = plugin.search("sh demo");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "shell:dir");
}

#[test]
fn search_plain_sh_opens_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let plugin = ShellPlugin;
    let results = plugin.search("sh");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "shell:dialog");
}
