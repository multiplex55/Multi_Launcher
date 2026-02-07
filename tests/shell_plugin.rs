use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::shell::{
    load_shell_cmds, save_shell_cmds, ShellCmdEntry, ShellPlugin, SHELL_CMDS_FILE,
};
use multi_launcher::{actions::Action, launcher::launch_action};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn load_shell_cmds_roundtrip() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![ShellCmdEntry {
        name: "test".into(),
        args: "echo hi".into(),
        preview_text: None,
        risk_level: None,
        icon: None,
        autocomplete: true,
        keep_open: false,
    }];
    save_shell_cmds(SHELL_CMDS_FILE, &entries).unwrap();
    let loaded = load_shell_cmds(SHELL_CMDS_FILE).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "test");
    assert_eq!(loaded[0].args, "echo hi");
    assert!(loaded[0].autocomplete);
    assert!(!loaded[0].keep_open);
}

#[test]
fn search_named_command_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![ShellCmdEntry {
        name: "demo".into(),
        args: "dir".into(),
        preview_text: None,
        risk_level: None,
        icon: None,
        autocomplete: true,
        keep_open: false,
    }];
    save_shell_cmds(SHELL_CMDS_FILE, &entries).unwrap();

    let plugin = ShellPlugin;
    let results = plugin.search("sh demo");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "shell:dir");
}

#[test]
fn search_respects_autocomplete_flag() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![ShellCmdEntry {
        name: "demo".into(),
        args: "dir".into(),
        preview_text: None,
        risk_level: None,
        icon: None,
        autocomplete: false,
        keep_open: false,
    }];
    save_shell_cmds(SHELL_CMDS_FILE, &entries).unwrap();

    let plugin = ShellPlugin;
    let results = plugin.search("sh demo");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "shell:demo");
}

#[test]
fn search_keep_open_uses_shell_keep_prefix() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![ShellCmdEntry {
        name: "demo".into(),
        args: "dir".into(),
        preview_text: None,
        risk_level: None,
        icon: None,
        autocomplete: true,
        keep_open: true,
    }];
    save_shell_cmds(SHELL_CMDS_FILE, &entries).unwrap();

    let plugin = ShellPlugin;
    let results = plugin.search("sh demo");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "shell_keep:dir");
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

#[test]
fn search_add_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = ShellPlugin;
    let results = plugin.search("sh add greet echo hi");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "shell:add:greet|echo hi");
}

#[test]
fn rm_lists_matching_commands() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![
        ShellCmdEntry {
            name: "a".into(),
            args: "cmd_a".into(),
            preview_text: None,
            risk_level: None,
            icon: None,
            autocomplete: true,
            keep_open: false,
        },
        ShellCmdEntry {
            name: "b".into(),
            args: "cmd_b".into(),
            preview_text: None,
            risk_level: None,
            icon: None,
            autocomplete: true,
            keep_open: false,
        },
    ];
    save_shell_cmds(SHELL_CMDS_FILE, &entries).unwrap();

    let plugin = ShellPlugin;
    let results = plugin.search("sh rm a");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "shell:remove:a");
}

#[test]
fn list_returns_saved_commands() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![ShellCmdEntry {
        name: "x".into(),
        args: "dir".into(),
        preview_text: None,
        risk_level: None,
        icon: None,
        autocomplete: true,
        keep_open: false,
    }];
    save_shell_cmds(SHELL_CMDS_FILE, &entries).unwrap();

    let plugin = ShellPlugin;
    let results = plugin.search("sh list");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "x");
    assert_eq!(results[0].action, "shell:dir");
}

#[test]
fn launch_actions_modify_file() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_shell_cmds(SHELL_CMDS_FILE, &[]).unwrap();
    let add = Action {
        label: String::new(),
        desc: String::new(),
        action: "shell:add:test|dir".into(),
        args: None,
        preview_text: None,
        risk_level: None,
        icon: None,
    };
    launch_action(&add).unwrap();
    let list = load_shell_cmds(SHELL_CMDS_FILE).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "test");

    let rm = Action {
        label: String::new(),
        desc: String::new(),
        action: "shell:remove:test".into(),
        args: None,
        preview_text: None,
        risk_level: None,
        icon: None,
    };
    launch_action(&rm).unwrap();
    let list = load_shell_cmds(SHELL_CMDS_FILE).unwrap();
    assert!(list.is_empty());
}
