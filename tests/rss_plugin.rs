use std::fs;

use httpmock::prelude::*;
use multi_launcher::actions::rss;
use multi_launcher::plugins::rss::storage::{ensure_config_dir, FeedCache, FeedsFile, StateFile};
use serial_test::serial;

/// Ensure a generic RSS feed can be added, listed and refreshed.
///
/// A mock HTTP server serves a static Atom feed so the test runs deterministically
/// and without external network access.
#[test]
#[serial]
fn add_list_and_refresh_feed() {
    // Clear any existing rss configuration to make the test repeatable.
    let _ = fs::remove_dir_all(ensure_config_dir());

    let server = MockServer::start();
    let feed_body = r#"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<feed xmlns=\"http://www.w3.org/2005/Atom\">
  <id>test</id>
  <title>Test Feed</title>
  <updated>2024-01-01T00:00:00Z</updated>
  <entry>
    <id>1</id>
    <title>First</title>
    <updated>2024-01-01T00:00:00Z</updated>
  </entry>
</feed>"#;
    let _m = server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200)
            .header("content-type", "application/atom+xml")
            .body(feed_body);
    });

    let feed_url = format!("{}/feed.xml", server.base_url());
    rss::run(&format!("add {feed_url}")).unwrap();

    // Load added feed to obtain the generated id.
    let feeds = FeedsFile::load();
    assert_eq!(feeds.feeds.len(), 1);
    let feed_id = feeds.feeds[0].id.clone();

    // Listing feeds should succeed and keep the feed in storage.
    rss::run("list feeds").unwrap();
    let feeds = FeedsFile::load();
    assert!(feeds.feeds.iter().any(|f| f.id == feed_id));

    // Refresh to retrieve feed items from the mock server.
    rss::run(&format!("refresh {feed_id}")).unwrap();
    let state = StateFile::load();
    let entry = state.feeds.get(&feed_id).expect("feed state exists");
    assert!(entry.last_guid.is_some());
    let cache = FeedCache::load(&feed_id);
    assert_eq!(cache.items.len(), 1);
}

/// Verify that adding a YouTube channel feed by its direct feed URL succeeds.
#[test]
#[serial]
fn add_youtube_channel_feed() {
    // Remove existing config to isolate the test.
    let _ = fs::remove_dir_all(ensure_config_dir());

    let url = "https://www.youtube.com/feeds/videos.xml?channel_id=abc123";
    rss::run(&format!("add {url}")).unwrap();
    let feeds = FeedsFile::load();
    assert_eq!(feeds.feeds.len(), 1);
    assert_eq!(feeds.feeds[0].url, url);
}
