use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::macros::{
    run_macro, save_macros, take_step_messages, MacroEntry, MacroStep, MacrosPlugin, MACROS_FILE,
};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn run_macro_executes_steps() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let macros = vec![MacroEntry {
        label: "demo".into(),
        desc: String::new(),
        auto_delay_ms: None,
        steps: vec![
            MacroStep {
                label: "first".into(),
                command: "history:clear".into(),
                args: None,
                delay_ms: 0,
            },
            MacroStep {
                label: "second".into(),
                command: "history:clear".into(),
                args: None,
                delay_ms: 0,
            },
        ],
    }];
    save_macros(MACROS_FILE, &macros).unwrap();

    run_macro("demo").unwrap();
    let msgs = take_step_messages();
    assert_eq!(msgs, vec!["Step 1: first", "Step 2: second"]);
}

#[test]
fn macros_file_change_reload() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_macros(
        MACROS_FILE,
        &[MacroEntry {
            label: "one".into(),
            desc: String::new(),
            auto_delay_ms: None,
            steps: Vec::new(),
        }],
    )
    .unwrap();

    let plugin = MacrosPlugin::new();
    let results = plugin.search("macro list");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "one");

    save_macros(
        MACROS_FILE,
        &[MacroEntry {
            label: "two".into(),
            desc: String::new(),
            auto_delay_ms: None,
            steps: Vec::new(),
        }],
    )
    .unwrap();
    // Wait for the watcher to pick up the changes. The callback may fire
    // asynchronously, so poll for the updated label with a timeout.
    for _ in 0..50 {
        sleep(Duration::from_millis(100));
        let results = plugin.search("macro list");
        if results.len() == 1 && results[0].label == "two" {
            return;
        }
    }
    panic!("macros file did not reload");
}
