use multi_launcher::usage::{load_usage, save_usage};
use std::collections::HashMap;
use tempfile::tempdir;

#[test]
fn save_then_load_usage() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("usage.json");
    let mut usage = HashMap::new();
    usage.insert("first".to_string(), 3);
    usage.insert("second".to_string(), 1);
    save_usage(path.to_str().unwrap(), &usage).unwrap();

    let loaded = load_usage(path.to_str().unwrap()).unwrap();
    assert_eq!(loaded.get("first"), Some(&3));
    assert_eq!(loaded.get("second"), Some(&1));
    assert_eq!(loaded.len(), 2);
}
