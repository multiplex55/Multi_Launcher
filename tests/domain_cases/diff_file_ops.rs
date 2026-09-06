use multi_launcher::diff::file_ops::*;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

#[test]
fn copies_file_and_plans_overwrite() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    fs::write(a.path().join("x"), "new").unwrap();
    fs::write(b.path().join("x"), "old value").unwrap();
    let p = plan_copy(
        a.path(),
        b.path(),
        CopyDirection::LeftToRight,
        [PathBuf::from("x")],
        7,
    )
    .unwrap();
    assert_eq!(p.totals.overwrites, 1);
    assert!(p.requires_confirmation());
    let r = execute_copy(&p, &HashSet::new(), &AtomicBool::new(false));
    assert!(matches!(r.items[0].outcome, ItemOutcome::Overwritten));
    assert_eq!(fs::read_to_string(b.path().join("x")).unwrap(), "new");
}

#[test]
fn recursive_copy_deduplicates_nested_selection() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    fs::create_dir(a.path().join("d")).unwrap();
    fs::write(a.path().join("d/x"), "x").unwrap();
    let p = plan_copy(
        a.path(),
        b.path(),
        CopyDirection::LeftToRight,
        [PathBuf::from("d"), PathBuf::from("d/x")],
        1,
    )
    .unwrap();
    assert_eq!(p.copies.len(), 1);
    assert_eq!(p.directories.len(), 1);
    let report = execute_copy(&p, &HashSet::new(), &AtomicBool::new(false));
    assert!(
        report.items.iter().all(|item| !matches!(
            item.outcome,
            ItemOutcome::Failed(_) | ItemOutcome::Cancelled
        )),
        "recursive copy report: {report:#?}"
    );
    assert!(b.path().join("d/x").is_file());
}

#[test]
fn rejects_escape_spellings_and_destination_conflict() {
    for bad in ["../x", "/tmp/x", "C:/x", r"dir\x"] {
        assert!(validate_relative(Path::new(bad)).is_err(), "{bad}");
    }
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    fs::write(a.path().join("x"), "x").unwrap();
    fs::create_dir(b.path().join("x")).unwrap();
    let p = plan_copy(
        a.path(),
        b.path(),
        CopyDirection::LeftToRight,
        [PathBuf::from("x")],
        1,
    )
    .unwrap();
    assert_eq!(p.totals.conflicts, 1);
    assert!(p.copies.is_empty());
}

#[derive(Default)]
struct FakeTrash {
    recycle: std::sync::atomic::AtomicUsize,
    permanent: std::sync::atomic::AtomicUsize,
    fail: bool,
}
impl TrashBackend for FakeTrash {
    fn recycle(&self, _: &Path) -> Result<(), String> {
        self.recycle
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.fail { Err("no".into()) } else { Ok(()) }
    }
    fn permanently_delete(&self, _: &Path) -> Result<(), String> {
        self.permanent
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}
#[test]
fn recycle_failure_never_falls_back_to_permanent_delete() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("x"), "x").unwrap();
    let p = plan_delete(
        d.path(),
        "left",
        [PathBuf::from("x")],
        3,
        DeleteMode::Recycle,
    )
    .unwrap();
    let backend = FakeTrash {
        fail: true,
        ..Default::default()
    };
    let r = execute_delete(&p, &HashSet::new(), &backend, &AtomicBool::new(false));
    assert!(matches!(r.items[0].outcome, ItemOutcome::Failed(_)));
    assert_eq!(backend.recycle.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        backend.permanent.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert!(d.path().join("x").exists());
}

#[test]
fn right_to_left_plan_retains_direction_and_executes_previewed_object() {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    fs::write(right.path().join("from-right"), "content").unwrap();
    let preview = plan_copy(
        right.path(),
        left.path(),
        CopyDirection::RightToLeft,
        [PathBuf::from("from-right")],
        42,
    )
    .unwrap();
    let executed = preview.clone();
    assert_eq!(executed, preview);
    let report = execute_copy(&executed, &HashSet::new(), &AtomicBool::new(false));
    assert!(matches!(report.items[0].outcome, ItemOutcome::Copied));
    assert_eq!(
        fs::read_to_string(left.path().join("from-right")).unwrap(),
        "content"
    );
}

#[test]
fn delete_normalizes_parent_child_and_dirty_descendant_blocks_parent() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("folder")).unwrap();
    fs::write(root.path().join("folder/dirty"), "unsaved on screen").unwrap();
    let plan = plan_delete(
        root.path(),
        "Left",
        [PathBuf::from("folder"), PathBuf::from("folder/dirty")],
        8,
        DeleteMode::Recycle,
    )
    .unwrap();
    assert_eq!(plan.items.len(), 1);
    let backend = FakeTrash::default();
    let dirty = HashSet::from([root.path().join("folder/dirty")]);
    let report = execute_delete(&plan, &dirty, &backend, &AtomicBool::new(false));
    assert!(
        matches!(&report.items[0].outcome, ItemOutcome::Failed(message)
        if message.contains("unsaved Diff changes"))
    );
    assert_eq!(backend.recycle.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn gui_spawn_boundary_rejects_permanent_deletion() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("x"), "x").unwrap();
    let plan = plan_delete(
        root.path(),
        "Left",
        [PathBuf::from("x")],
        1,
        DeleteMode::Permanent,
    )
    .unwrap();
    let result = spawn_recycle_delete(
        plan,
        HashSet::new(),
        std::sync::Arc::new(FakeTrash::default()),
    );
    assert!(result.is_err());
    assert!(root.path().join("x").exists());
}
