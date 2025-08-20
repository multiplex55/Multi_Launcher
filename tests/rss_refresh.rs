use std::fs;

use httpmock::prelude::*;
use multi_launcher::actions::rss;
use multi_launcher::plugins::rss::storage::{ensure_config_dir, FeedConfig, FeedsFile, StateFile};
use serial_test::serial;

#[test]
#[serial]
fn refresh_polls_even_when_not_due() {
    // Ensure a clean config directory
    let _ = fs::remove_dir_all(ensure_config_dir());

    let server = MockServer::start();
    let feed_body = r#"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<feed xmlns=\"http://www.w3.org/2005/Atom\">
  <id>test</id>
  <title>Test</title>
  <updated>2023-01-02T00:00:00Z</updated>
  <entry>
    <id>1</id>
    <title>First</title>
    <updated>2023-01-01T00:00:00Z</updated>
  </entry>
</feed>"#;

    let m = server.mock(|when, then| {
        when.method(GET).path("/feed");
        then.status(200)
            .header("content-type", "application/atom+xml")
            .body(feed_body);
    });

    let mut feeds = FeedsFile::default();
    feeds.feeds.push(FeedConfig {
        id: "f".into(),
        url: format!("{}/feed", server.base_url()),
        title: None,
        group: None,
        last_poll: None,
        next_poll: Some(9_999_999_999),
        cadence: None,
    });
    feeds.save().unwrap();
    StateFile::default().save().unwrap();

    rss::run("refresh f").unwrap();

    m.assert();
}
