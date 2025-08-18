use std::fs;

use multi_launcher::actions::rss;
use multi_launcher::plugins::rss::storage::{FeedConfig, FeedsFile};

#[test]
fn group_add_mv_rm_updates_feeds() {
    let _ = fs::remove_dir_all("config/rss");
    let mut feeds = FeedsFile::default();
    feeds.feeds.push(FeedConfig {
        id: "f".into(),
        url: "http://example.com".into(),
        title: None,
        group: Some("g1".into()),
        last_poll: None,
        next_poll: None,
        cadence: None,
    });
    feeds.groups.push("g1".into());
    feeds.save().unwrap();

    rss::run("group:add g2").unwrap();
    let mut feeds = FeedsFile::load();
    assert!(feeds.groups.contains(&"g2".to_string()));

    rss::run("group:mv g1 g3").unwrap();
    feeds = FeedsFile::load();
    assert!(!feeds.groups.contains(&"g1".to_string()));
    assert!(feeds.groups.contains(&"g3".to_string()));
    assert_eq!(feeds.feeds[0].group.as_deref(), Some("g3"));

    rss::run("group:rm g3").unwrap();
    feeds = FeedsFile::load();
    assert!(!feeds.groups.contains(&"g3".to_string()));
    assert!(feeds.feeds[0].group.is_none());
}
