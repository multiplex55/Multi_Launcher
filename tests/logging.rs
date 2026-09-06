use std::{env, fs, process::Command};

use serial_test::serial;
use tempfile::tempdir;

const CHILD_CASE: &str = "MULTI_LAUNCHER_LOGGING_TEST_CHILD";
const CHILD_PATH: &str = "MULTI_LAUNCHER_LOGGING_TEST_PATH";

fn run_in_fresh_process(test_name: &str, case: &str, path: &std::path::Path) {
    let status = Command::new(env::current_exe().expect("resolve logging test executable"))
        .args(["--exact", test_name, "--nocapture"])
        .env(CHILD_CASE, case)
        .env(CHILD_PATH, path)
        .env_remove("RUST_LOG")
        .status()
        .expect("run isolated logging test process");
    assert!(status.success(), "isolated logging test process failed");
}

#[test]
#[serial]
fn writes_log_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("log.txt");

    if env::var(CHILD_CASE).as_deref() == Ok("file") {
        let configured_path = env::var_os(CHILD_PATH).expect("child log path");
        let configured_path = std::path::PathBuf::from(configured_path);
        let guard = multi_launcher::logging::init(true, Some(configured_path.clone()))
            .expect("install file logging subscriber");
        tracing::info!("test");
        drop(guard);
        let contents = fs::read_to_string(configured_path).unwrap();
        assert!(contents.contains("test"));
        return;
    }

    run_in_fresh_process("writes_log_file", "file", &path);
    assert!(path.exists(), "log file was not created");
    let contents = fs::read_to_string(path).unwrap();
    assert!(contents.contains("test"));
}

#[test]
#[serial]
fn init_without_file_creates_no_log() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("log.txt");

    if env::var(CHILD_CASE).as_deref() == Ok("console") {
        assert!(multi_launcher::logging::init(false, None).is_none());
        tracing::info!("test");
        let configured_path = env::var_os(CHILD_PATH).expect("child log path");
        assert!(!std::path::Path::new(&configured_path).exists());
        return;
    }

    run_in_fresh_process("init_without_file_creates_no_log", "console", &path);
    assert!(!path.exists(), "log file should not be created");
}
