use multi_launcher::plugins::rss::RssPlugin;
use multi_launcher::plugin::Plugin;

#[test]
fn rss_command_discovery() {
    let plugin = RssPlugin::default();
    let acts = plugin.search("rss");
    let labels: Vec<_> = acts.iter().map(|a| a.label.as_str()).collect();
    assert!(labels.contains(&"rss add"));
    assert!(labels.contains(&"rss list"));
}

#[test]
fn rss_list_subcommand() {
    let plugin = RssPlugin::default();
    let acts = plugin.search("rss list");
    let labels: Vec<_> = acts.iter().map(|a| a.label.as_str()).collect();
    assert!(labels.contains(&"rss list groups"));
    assert!(labels.contains(&"rss list feeds"));
}
