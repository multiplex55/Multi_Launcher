use httpmock::prelude::*;
use multi_launcher::plugins::rss::poller::Poller;
use multi_launcher::plugins::rss::storage::{FeedConfig, StateFile};

#[test]
fn poller_sets_last_read_published_on_first_poll() {
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
  <entry>
    <id>2</id>
    <title>Second</title>
    <updated>2023-01-02T00:00:00Z</updated>
  </entry>
</feed>"#;

    let _m = server.mock(|when, then| {
        when.method(GET).path("/feed");
        then.status(200)
            .header("content-type", "application/atom+xml")
            .body(feed_body);
    });

    let mut feed = FeedConfig {
        id: "f".into(),
        url: format!("{}/feed", server.base_url()),
        title: None,
        group: None,
        last_poll: None,
        next_poll: None,
        cadence: None,
    };
    let mut state = StateFile::default();
    let poller = Poller::new().unwrap();
    let items = poller
        .poll_feed(&mut feed, &mut state, true, false)
        .unwrap();
    assert!(items.is_empty());
    let entry = state.feeds.get("f").unwrap();
    assert!(entry.last_read_published.is_some());
    assert_eq!(entry.unread, 0);
}
