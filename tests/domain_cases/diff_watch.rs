use multi_launcher::diff::watch::*;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn tag(generation: u64) -> WatchTag {
    WatchTag {
        workspace: 1,
        view: 2,
        generation,
    }
}

#[test]
fn coalesces_bursts_and_rejects_stale_generations() {
    let now = Instant::now();
    let mut c = EventCoalescer::new(Duration::from_millis(50));
    for path in ["/root/a.tmp", "/root/a"] {
        c.push(
            now,
            WatchEvent {
                tag: tag(1),
                identity_path: "/root".into(),
                paths: vec![path.into()],
                scope: WatchScope::Root,
            },
        );
    }
    assert!(
        c.drain_ready(now + Duration::from_millis(49), tag(1))
            .is_empty()
    );
    let ready = c.drain_ready(now + Duration::from_millis(50), tag(1));
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].paths.len(), 2);
    c.push(
        now,
        WatchEvent {
            tag: tag(1),
            identity_path: "/root".into(),
            paths: vec!["/root/a".into()],
            scope: WatchScope::Root,
        },
    );
    assert!(
        c.drain_ready(now + Duration::from_secs(1), tag(2))
            .is_empty()
    );
}

#[test]
fn exact_own_save_is_suppressed_but_later_change_is_not() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a");
    std::fs::write(&p, "one").unwrap();
    let first = stat_identity(&p).unwrap();
    let mut doc = ExternalDocument::new(p.clone(), Some(first), 1, tag(1));
    std::fs::write(&p, "saved-content").unwrap();
    let saved = stat_identity(&p).unwrap();
    doc.record_save(saved);
    assert!(doc.observe().is_none());
    std::fs::write(&p, "external content with distinct size").unwrap();
    assert!(doc.observe().is_some());
    assert_eq!(doc.state, ExternalState::Reloading);
}

#[test]
fn dirty_and_removed_files_retain_buffer_state() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a");
    std::fs::write(&p, "one").unwrap();
    let mut doc = ExternalDocument::new(p.clone(), Some(stat_identity(&p).unwrap()), 7, tag(1));
    doc.dirty = true;
    std::fs::write(&p, "different longer").unwrap();
    assert!(doc.observe().is_none());
    assert_eq!(doc.state, ExternalState::Conflict);
    std::fs::remove_file(&p).unwrap();
    assert!(doc.observe().is_none());
    assert!(matches!(doc.state, ExternalState::Missing(_)));
    assert!(doc.dirty);
}

#[test]
fn stale_reload_and_smallest_subtree() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a");
    std::fs::write(&p, "one").unwrap();
    let mut doc = ExternalDocument::new(p.clone(), None, 3, tag(1));
    let ticket = doc.ticket();
    let loaded = ExternalDocument::load(&ticket).unwrap();
    doc.revision = 4;
    assert!(!doc.accept_reload(&ticket, &loaded));
    assert_eq!(
        affected_subtree(
            PathBuf::from("/r").as_path(),
            &["/r/a/x.txt".into(), "/r/a/y/z.txt".into()]
        ),
        Some("a".into())
    );
}

#[test]
fn injected_folder_event_invalidates_smallest_affected_subtree() {
    let d = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut runtime = ViewWatchRuntime::folder(tag(4), d.path().into(), other.path().into());
    runtime.inject(
        now,
        WatchEvent {
            tag: tag(4),
            identity_path: d.path().into(),
            paths: vec![
                d.path().join("src/a.txt"),
                d.path().join("src/nested/b.txt"),
            ],
            scope: WatchScope::Root,
        },
    );
    assert!(
        runtime
            .poll(now + Duration::from_millis(119), [false; 2])
            .is_empty()
    );
    let actions = runtime.poll(now + Duration::from_millis(120), [false; 2]);
    assert!(
        matches!(&actions[..], [ViewWatchAction::FolderChanged { subtree, .. }] if subtree == &PathBuf::from("src"))
    );
}

#[test]
fn injected_clean_and_dirty_text_changes_are_arbitrated_without_data_loss() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("left.txt");
    std::fs::write(&p, "before").unwrap();
    let now = Instant::now();
    let mut clean = ViewWatchRuntime::text(tag(5), Some(p.clone()), None);
    std::fs::write(&p, "after, externally").unwrap();
    clean.inject(
        now,
        WatchEvent {
            tag: tag(5),
            identity_path: p.clone(),
            paths: vec![p.clone()],
            scope: WatchScope::File,
        },
    );
    let actions = clean.poll(now + Duration::from_secs(1), [false; 2]);
    assert!(
        matches!(&actions[..], [ViewWatchAction::TextReload { side: multi_launcher::diff::model::DiffSide::Left, loaded }] if loaded.text() == Some("after, externally"))
    );

    let mut dirty = ViewWatchRuntime::text(tag(6), Some(p.clone()), None);
    std::fs::write(&p, "a second external version").unwrap();
    dirty.inject(
        now,
        WatchEvent {
            tag: tag(6),
            identity_path: p.clone(),
            paths: vec![p.clone()],
            scope: WatchScope::File,
        },
    );
    let actions = dirty.poll(now + Duration::from_secs(1), [true, false]);
    assert!(matches!(
        &actions[..],
        [ViewWatchAction::TextConflict { .. }]
    ));
}

#[test]
fn obsolete_root_and_generation_events_are_ignored_and_binary_refreshes_once() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.bin");
    std::fs::write(&p, [0, 1]).unwrap();
    let now = Instant::now();
    let mut binary = ViewWatchRuntime::binary(tag(8), Some(p.clone()), None);
    for event in [
        WatchEvent {
            tag: tag(7),
            identity_path: p.clone(),
            paths: vec![p.clone()],
            scope: WatchScope::File,
        },
        WatchEvent {
            tag: tag(8),
            identity_path: d.path().join("obsolete.bin"),
            paths: vec![p.clone()],
            scope: WatchScope::File,
        },
    ] {
        binary.inject(now, event);
    }
    assert!(
        binary
            .poll(now + Duration::from_secs(1), [false; 2])
            .is_empty()
    );
    binary.inject(
        now,
        WatchEvent {
            tag: tag(8),
            identity_path: p.clone(),
            paths: vec![p],
            scope: WatchScope::File,
        },
    );
    assert!(matches!(
        &binary.poll(now + Duration::from_secs(1), [false; 2])[..],
        [ViewWatchAction::BinaryRefresh]
    ));
}
