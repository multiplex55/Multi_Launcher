use std::fs;
use tempfile::tempdir;

// Ensure index_paths returns actions for each file in directory tree
#[test]
fn indexer_indexes_files_recursively() {
    let dir = tempdir().expect("failed to create temp dir");
    let file1 = dir.path().join("file1.txt");
    let file2 = dir.path().join("file2.log");
    let subdir = dir.path().join("nested");
    fs::create_dir(&subdir).expect("create subdir");
    let file3 = subdir.join("file3.md");

    fs::write(&file1, b"one").expect("write file1");
    fs::write(&file2, b"two").expect("write file2");
    fs::write(&file3, b"three").expect("write file3");

    let paths = vec![dir.path().to_string_lossy().to_string()];
    let actions = multi_launcher::indexer::index_paths(&paths).expect("indexing failed");
    assert_eq!(actions.len(), 3);

    let expected = [file1, file2, file3];
    for path in expected.iter() {
        let canonical = fs::canonicalize(path).expect("canonical path");
        let label = canonical.file_name().unwrap().to_str().unwrap();
        let display = canonical.display().to_string();
        assert!(actions.iter().any(|a| a.label == label
            && a.action == display
            && a.desc == display
            && a.args.is_none()));
    }
}

#[test]
fn indexer_batches_dedupes_and_honors_max_items() {
    let dir = tempdir().expect("failed to create temp dir");
    let one = dir.path().join("one.txt");
    let two = dir.path().join("two.txt");
    let three = dir.path().join("three.txt");
    fs::write(&one, b"1").expect("write one");
    fs::write(&two, b"2").expect("write two");
    fs::write(&three, b"3").expect("write three");

    let same_root = dir.path().to_string_lossy().to_string();
    let paths = vec![same_root.clone(), same_root];
    let mut iter = multi_launcher::indexer::index_paths_batched(
        &paths,
        multi_launcher::indexer::IndexOptions {
            batch_size: 2,
            max_items: 2,
        },
    );

    let first = iter.next().expect("first batch").expect("first ok");
    assert_eq!(first.len(), 2);
    assert!(iter.next().is_none(), "max_items should stop iteration");

    let mut seen = std::collections::HashSet::new();
    for action in first {
        assert!(seen.insert(action.action), "deduped paths only");
    }
}

// Ensure indexing a missing path returns an error
#[test]
fn indexer_errors_on_missing_path() {
    let dir = tempdir().expect("tempdir");
    let missing = dir.path().join("does_not_exist");
    let result = multi_launcher::indexer::index_paths(&[missing.to_string_lossy().into_owned()]);
    assert!(result.is_err());
}
