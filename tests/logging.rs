use std::{fs, path::PathBuf, process::Command, thread::sleep, time::Duration};

use tempfile::tempdir;

fn run_child(test: &str, path: &PathBuf) {
    let status = Command::new(std::env::current_exe().unwrap())
        .env("LOG_TEST_PATH", path)
        .arg("--ignored")
        .arg(test)
        .status()
        .expect("spawn child");
    assert!(status.success());
}

#[test]
fn writes_log_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("log.txt");
    run_child("child_writes_log_file", &path);
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("test"));
}

#[test]
fn init_without_file_creates_no_log() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("log.txt");
    run_child("child_init_without_file", &path);
    assert!(!path.exists(), "log file should not be created");
}

#[test]
#[ignore]
fn child_writes_log_file() {
    let path = PathBuf::from(std::env::var("LOG_TEST_PATH").unwrap());
    multi_launcher::logging::init(true, Some(path.clone()));
    tracing::info!("test");
    // Give the async writer time to flush before the process exits.
    sleep(Duration::from_millis(100));
}

#[test]
#[ignore]
fn child_init_without_file() {
    multi_launcher::logging::init(false, None);
    tracing::info!("test");
    sleep(Duration::from_millis(100));
}
