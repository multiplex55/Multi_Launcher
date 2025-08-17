use std::{fs, thread::sleep, time::Duration};

use serial_test::serial;
use tempfile::tempdir;

#[test]
#[serial]
fn writes_log_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("log.txt");

    multi_launcher::logging::init(true, Some(path.clone()));
    tracing::info!("test");

    sleep(Duration::from_millis(100));

    assert!(path.exists(), "log file was not created");
    let contents = fs::read_to_string(path).unwrap();
    assert!(contents.contains("test"));
}

#[test]
#[serial]
fn init_without_file_creates_no_log() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("log.txt");

    multi_launcher::logging::init(false, None);
    tracing::info!("test");

    sleep(Duration::from_millis(100));

    assert!(!path.exists(), "log file should not be created");
}
