use std::fs;

use multi_launcher::actions::rss;
use multi_launcher::plugins::rss::storage;
use tempfile::NamedTempFile;

#[test]
fn import_export_opml() {
    // clean RSS config directory
    let base = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("rss");
    let _ = fs::remove_dir_all(&base);

    let opml = r#"<?xml version="1.0"?>
<opml version="1.0">
<body>
  <outline text="Group1">
    <outline text="Feed One" xmlUrl="https://example.com/1" />
  </outline>
  <outline text="Feed Two" xmlUrl="https://example.com/2" />
  <outline text="Feed Two" xmlUrl="https://example.com/2" />
  <outline text="Invalid" />
</body>
</opml>"#;

    let file = NamedTempFile::new().unwrap();
    fs::write(file.path(), opml).unwrap();

    rss::run(&format!("import {}", file.path().display())).unwrap();

    let feeds = storage::FeedsFile::load();
    assert_eq!(feeds.feeds.len(), 2);
    assert!(feeds.groups.contains(&"Group1".to_string()));
    let feed1 = feeds
        .feeds
        .iter()
        .find(|f| f.url == "https://example.com/1")
        .unwrap();
    assert_eq!(feed1.group.as_deref(), Some("Group1"));

    let export = NamedTempFile::new().unwrap();
    rss::run(&format!("export {}", export.path().display())).unwrap();
    let exported = fs::read_to_string(export.path()).unwrap();
    assert!(exported.contains("https://example.com/1"));
    assert!(exported.contains("Group1"));
}

