use multi_launcher::diff::folder_compare::*;
use multi_launcher::diff::folder_scan::ScanRules;
use std::path::Path;
use std::time::{Duration, SystemTime};

fn side(kind: EntryKind, size: u64, seconds: u64) -> EntrySide {
    EntrySide {
        path: "unused".into(),
        metadata: Some(EntryMetadata {
            kind,
            size,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)),
            identity: None,
        }),
        error: None,
    }
}

#[test]
fn normalized_pairing_case_policy_and_traversal() {
    let mut model = FolderModel::default();
    model
        .upsert(
            Path::new("nested/file.TXT"),
            side(EntryKind::File, 3, 1),
            true,
            PathKeyPolicy::Insensitive,
            Duration::ZERO,
        )
        .unwrap();
    model
        .upsert(
            Path::new("nested/file.txt"),
            side(EntryKind::File, 3, 1),
            false,
            PathKeyPolicy::Insensitive,
            Duration::ZERO,
        )
        .unwrap();
    assert_eq!(model.entries.len(), 1);
    assert!(path_key(Path::new("../escape"), PathKeyPolicy::Sensitive).is_err());
    assert_eq!(
        path_key(Path::new("a/b"), PathKeyPolicy::Sensitive).unwrap(),
        "a/b"
    );
}

#[test]
fn quick_status_distinguishes_content_pending_and_timestamp_tolerance() {
    let left = side(EntryKind::File, 10, 10);
    let right = side(EntryKind::File, 10, 11);
    assert_eq!(
        fast_status(Some(&left), Some(&right), Duration::from_secs(1)),
        FolderStatus::PendingContentComparison
    );
    assert_eq!(
        fast_status(Some(&left), Some(&right), Duration::ZERO),
        FolderStatus::RightNewer
    );
    assert_eq!(
        fast_status(Some(&left), None, Duration::ZERO),
        FolderStatus::LeftOnly
    );
}

#[test]
fn scan_patterns_use_basename_and_complete_normalized_path() {
    let rules = ScanRules {
        includes: vec![],
        excludes: vec!["*.tmp".into(), "build/*.bak".into()],
    };
    assert!(!rules.permits(Path::new("hidden/a.tmp"), false));
    assert!(!rules.permits(Path::new("build/a.bak"), false));
    assert!(rules.permits(Path::new("other/a.bak"), false));
}

#[test]
fn display_filter_does_not_mutate_retained_model() {
    let mut model = FolderModel::default();
    model
        .upsert(
            Path::new("only"),
            side(EntryKind::File, 1, 1),
            true,
            PathKeyPolicy::Sensitive,
            Duration::ZERO,
        )
        .unwrap();
    let revision = model.revision;
    assert_eq!(model.visible(DisplayFilter::RightOnly, "").len(), 0);
    assert_eq!(model.entries.len(), 1);
    assert_eq!(model.revision, revision);
}
