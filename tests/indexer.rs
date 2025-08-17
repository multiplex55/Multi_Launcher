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
        let label = path.file_name().unwrap().to_str().unwrap();
        let display = path.display().to_string();
        assert!(actions.iter().any(|a| a.label == label && a.action == display && a.desc == display && a.args.is_none()));
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

