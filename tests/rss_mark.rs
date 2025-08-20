use std::fs;

use multi_launcher::actions::rss;
use multi_launcher::plugins::rss::storage::{
    self, ensure_config_dir, CachedItem, FeedCache, FeedConfig, FeedsFile, StateFile,
};
use serial_test::serial;

fn setup_feed_with_cache(items: Vec<CachedItem>) {
    let _ = fs::remove_dir_all(ensure_config_dir());
    let mut feeds = FeedsFile::default();
    feeds.feeds.push(FeedConfig {
        id: "f".into(),
        url: "http://example.com".into(),
        title: None,
        group: None,
        last_poll: None,
        next_poll: None,
        cadence: None,
    });
    feeds.save().unwrap();
    let mut cache = FeedCache::default();
    cache.items = items;
    cache.save("f").unwrap();
}

#[test]
#[serial]
fn open_marks_items_read() {
    setup_feed_with_cache(vec![CachedItem {
        guid: "a".into(),
        title: "First".into(),
        link: None,
        timestamp: Some(1),
    }]);
    let mut state = StateFile::default();
    state.feeds.insert(
        "f".into(),
        storage::FeedState {
            unread: 1,
            ..Default::default()
        },
    );
    state.save().unwrap();

    rss::run("open f --n 1").unwrap();

    let state = StateFile::load();
    let entry = state.feeds.get("f").unwrap();
    assert!(entry.read.contains("a"));
    assert_eq!(entry.unread, 0);
}

#[test]
#[serial]
fn open_copy_marks_items_read() {
    setup_feed_with_cache(vec![CachedItem {
        guid: "a".into(),
        title: "First".into(),
        link: Some("http://example.com".into()),
        timestamp: Some(1),
    }]);
    let mut state = StateFile::default();
    state.feeds.insert(
        "f".into(),
        storage::FeedState {
            unread: 1,
            ..Default::default()
        },
    );
    state.save().unwrap();

    rss::run("open f --n 1 --copy").unwrap();

    let state = StateFile::load();
    let entry = state.feeds.get("f").unwrap();
    assert!(entry.read.contains("a"));
    assert_eq!(entry.unread, 0);
}

#[test]
#[serial]
fn mark_read_and_unread_updates_state() {
    setup_feed_with_cache(vec![
        CachedItem {
            guid: "a".into(),
            title: "A".into(),
            link: None,
            timestamp: Some(1),
        },
        CachedItem {
            guid: "b".into(),
            title: "B".into(),
            link: None,
            timestamp: Some(2),
        },
        CachedItem {
            guid: "c".into(),
            title: "C".into(),
            link: None,
            timestamp: Some(3),
        },
    ]);
    let mut state = StateFile::default();
    let mut entry = storage::FeedState {
        last_read_published: Some(1),
        unread: 1,
        ..Default::default()
    };
    entry.read.insert("a".into());
    entry.read.insert("c".into());
    state.feeds.insert("f".into(), entry);
    state.save().unwrap();

    rss::run("mark read f --through 1970-01-01T00:00:02Z").unwrap();
    let state = StateFile::load();
    let entry = state.feeds.get("f").unwrap();
    assert_eq!(entry.last_read_published, Some(2));
    assert!(!entry.read.contains("a"));
    assert!(entry.read.contains("c"));
    assert_eq!(entry.unread, 0);

    rss::run("mark unread f/c").unwrap();
    let state = StateFile::load();
    let entry = state.feeds.get("f").unwrap();
    assert!(!entry.read.contains("c"));
    assert_eq!(entry.unread, 1);
}
