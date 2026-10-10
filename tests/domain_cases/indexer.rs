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

#[test]
fn indexer_single_file_roots_preserve_order_and_deduplicate_canonically() {
    let dir = tempdir().expect("failed to create temp dir");
    let root_a = dir.path().join("a.txt");
    let root_b = dir.path().join("b.txt");
    fs::write(&root_a, b"A").expect("write A");
    fs::write(&root_b, b"B").expect("write B");
    let a = fs::canonicalize(root_a).expect("canonical A");
    let b = fs::canonicalize(root_b).expect("canonical B");
    let a = a.to_string_lossy().into_owned();
    let b = b.to_string_lossy().into_owned();

    let first_config = multi_launcher::indexer::coordinator::IndexConfig::new(
        vec![a.clone(), a.clone(), b.clone(), a.clone()],
        Some(2),
    );
    let reordered_config = multi_launcher::indexer::coordinator::IndexConfig::new(
        vec![b.clone(), b.clone(), a.clone(), b.clone()],
        Some(2),
    );
    assert_ne!(first_config, reordered_config);

    let collect = |roots: &[String]| {
        multi_launcher::indexer::index_paths_batched(
            roots,
            multi_launcher::indexer::IndexOptions {
                batch_size: 1,
                max_items: 2,
            },
        )
        .flat_map(Result::unwrap)
        .collect::<Vec<_>>()
    };
    let first = collect(first_config.roots());
    let reordered = collect(reordered_config.roots());
    let expected = |path: &str| {
        let path = std::path::Path::new(path);
        let display = path.display().to_string();
        multi_launcher::actions::Action {
            label: path.file_name().unwrap().to_string_lossy().into_owned(),
            desc: display.clone(),
            action: display,
            args: None,
        }
    };
    assert_eq!(first, [expected(&a), expected(&b)]);
    assert_eq!(reordered, [expected(&b), expected(&a)]);
}

// Ensure indexing a missing path returns an error
#[test]
fn indexer_errors_on_missing_path() {
    let dir = tempdir().expect("tempdir");
    let missing = dir.path().join("does_not_exist");
    let result = multi_launcher::indexer::index_paths(&[missing.to_string_lossy().into_owned()]);
    assert!(result.is_err());
}
