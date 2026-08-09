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
